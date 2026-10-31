//! Update checks against the GitHub releases of the project, and downloading the installer.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

/// The API address of the newest published release.
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/RoleplayHarness/InfiniteRoleplay/releases/latest";

/// The version this build was made from.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What can go wrong while checking for or fetching an update.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// GitHub could not be reached or answered with an error.
    #[error("could not reach GitHub: {0}")]
    Network(String),
    /// The answer was not a release description.
    #[error("unexpected answer from GitHub: {0}")]
    Format(String),
    /// The installer could not be stored or started.
    #[error("could not handle the installer: {0}")]
    Io(String),
}

/// A published release that is newer than the running app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The version without a leading `v`, such as `0.6.0`.
    pub version: String,
    /// The release page, for opening in a browser.
    pub page_url: String,
    /// The installer for this platform, if the release has one.
    pub asset: Option<Asset>,
}

/// A downloadable file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// The file name.
    pub name: String,
    /// Where to download it.
    pub url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// Split `1.2.3`, `v1.2.3`, or `1.2.3-rc.1` into numbers. Anything after the patch number
/// is ignored.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// Whether `latest` is a higher version than `current`. Unreadable versions are never newer.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

/// The installer to use on operating system `os` (as in `std::env::consts::OS`).
fn pick_asset(assets: Vec<ApiAsset>, os: &str) -> Option<Asset> {
    let suffix = match os {
        "windows" => ".exe",
        "linux" => ".appimage",
        _ => return None,
    };
    assets
        .into_iter()
        .find(|a| a.name.to_lowercase().ends_with(suffix))
        .map(|a| Asset {
            name: a.name,
            url: a.browser_download_url,
        })
}

fn http() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .user_agent(concat!("MasqueRole/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

/// Ask `url` (normally [`LATEST_RELEASE_URL`]) for the newest release. Returns it only if it
/// is newer than `current` and is neither a draft nor a pre-release.
pub async fn check(url: &str, current: &str, os: &str) -> Result<Option<Release>, UpdateError> {
    let response = http()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let release: ApiRelease = response
        .json()
        .await
        .map_err(|e| UpdateError::Format(e.to_string()))?;
    if release.draft || release.prerelease || !is_newer(current, &release.tag_name) {
        return Ok(None);
    }
    Ok(Some(Release {
        version: release.tag_name.trim_start_matches('v').to_owned(),
        page_url: release.html_url,
        asset: pick_asset(release.assets, os),
    }))
}

/// Download `asset` into `dir` and return the file's path.
pub async fn download(asset: &Asset, dir: &Path) -> Result<PathBuf, UpdateError> {
    // The name comes from the network; keep only its last component.
    let name = Path::new(&asset.name)
        .file_name()
        .ok_or_else(|| UpdateError::Format("the installer has no file name".into()))?;
    let bytes = http()?
        .get(&asset.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| UpdateError::Network(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    std::fs::create_dir_all(dir).map_err(|e| UpdateError::Io(e.to_string()))?;
    let path = dir.join(name);
    std::fs::write(&path, &bytes).map_err(|e| UpdateError::Io(e.to_string()))?;
    Ok(path)
}

/// Start the downloaded installer. The caller closes the app afterwards so the installer
/// can replace its files.
pub fn launch_installer(path: &Path) -> Result<(), UpdateError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)
            .map_err(|e| UpdateError::Io(e.to_string()))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).map_err(|e| UpdateError::Io(e.to_string()))?;
    }
    std::process::Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::Io(e.to_string()))
}

/// Open `url` in the default browser. Only `https://` addresses are opened.
pub fn open_page(url: &str) -> Result<(), UpdateError> {
    if !url.starts_with("https://") {
        return Err(UpdateError::Io("not an https address".into()));
    }
    // No shell is involved, so characters in the address cannot run anything.
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(is_newer("0.5.2", "v0.6.0"));
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("1.2.3", "1.2.4"));
        assert!(!is_newer("0.6.0", "0.6.0"));
        assert!(!is_newer("0.6.0", "v0.5.9"));
    }

    #[test]
    fn unreadable_versions_are_never_newer() {
        assert!(!is_newer("0.5.2", "nightly"));
        assert!(!is_newer("0.5.2", "1.0"));
        assert!(!is_newer("0.5.2", "1.0.0.1"));
        assert!(!is_newer("dev", "1.0.0"));
    }

    #[test]
    fn only_https_addresses_are_opened() {
        assert!(open_page("file:///etc/passwd").is_err());
        assert!(open_page("http://example.test").is_err());
    }

    #[test]
    fn a_suffix_after_the_patch_number_is_ignored() {
        assert_eq!(parse_version("v1.2.3-rc.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3+build5"), Some((1, 2, 3)));
    }

    fn assets() -> Vec<ApiAsset> {
        [
            "app_0.6.0_x64-setup.exe",
            "app_0.6.0_amd64.deb",
            "app.AppImage",
        ]
        .into_iter()
        .map(|name| ApiAsset {
            name: name.into(),
            browser_download_url: format!("https://example.test/{name}"),
        })
        .collect()
    }

    #[test]
    fn the_installer_matches_the_operating_system() {
        let windows = pick_asset(assets(), "windows").unwrap();
        assert_eq!(windows.name, "app_0.6.0_x64-setup.exe");
        let linux = pick_asset(assets(), "linux").unwrap();
        assert_eq!(linux.name, "app.AppImage");
        assert_eq!(pick_asset(assets(), "macos"), None);
    }

    /// Serve one canned HTTP response on a local port and return its address.
    async fn serve(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/release", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
        });
        url
    }

    const RELEASE: &str = r#"{"tag_name":"v0.6.0","html_url":"https://example.test/r","draft":false,
        "prerelease":false,"assets":[{"name":"setup.exe","browser_download_url":"https://example.test/setup.exe"}]}"#;

    #[tokio::test]
    async fn a_newer_release_is_reported_with_its_installer() {
        let url = serve(RELEASE).await;
        let release = check(&url, "0.5.2", "windows").await.unwrap().unwrap();
        assert_eq!(release.version, "0.6.0");
        assert_eq!(release.page_url, "https://example.test/r");
        assert_eq!(release.asset.unwrap().name, "setup.exe");
    }

    #[tokio::test]
    async fn the_same_version_is_not_an_update() {
        let url = serve(RELEASE).await;
        assert_eq!(check(&url, "0.6.0", "windows").await.unwrap(), None);
    }

    #[tokio::test]
    async fn pre_releases_are_ignored() {
        let url = serve(r#"{"tag_name":"v9.0.0","html_url":"x","prerelease":true}"#).await;
        assert_eq!(check(&url, "0.5.2", "windows").await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_answer_that_is_not_a_release_is_a_format_error() {
        let url = serve("[]").await;
        let error = check(&url, "0.5.2", "windows").await.unwrap_err();
        assert!(matches!(error, UpdateError::Format(_)), "{error}");
    }

    #[tokio::test]
    async fn download_stores_the_file_under_its_own_name_only() {
        let url = serve("installer bytes").await;
        let dir = std::env::temp_dir().join(format!("ir-update-{}", std::process::id()));
        let asset = Asset {
            name: "../escape.exe".into(),
            url,
        };
        let path = download(&asset, &dir).await.unwrap();
        assert_eq!(path, dir.join("escape.exe"));
        assert_eq!(std::fs::read(&path).unwrap(), b"installer bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
