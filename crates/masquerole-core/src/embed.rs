//! Turning text into vectors, so the story can be searched by meaning.
//!
//! [`MiniLm`] runs the small `all-MiniLM-L6-v2` sentence model on the CPU, inside the app.
//! Its files (about 90 MB) are downloaded once into the data directory, after the user turns
//! semantic memory on. [`HashEmbedder`] needs no files and is used in tests.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// What can go wrong while embedding text.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    /// The model files are missing or could not be read.
    #[error("the embedding model is not available: {0}")]
    Model(String),
    /// The model failed on some text.
    #[error("could not embed the text: {0}")]
    Run(String),
    /// The model files could not be downloaded.
    #[error("could not download the embedding model: {0}")]
    Download(String),
}

/// Something that turns texts into vectors of one fixed length.
pub trait Embedder: Send + Sync {
    /// A name that changes whenever the vectors do, so old vectors are not mixed with new
    /// ones.
    fn model(&self) -> &str;

    /// One unit-length vector per text, in the same order.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
}

/// Name of the bundled sentence model.
pub const MINILM_NAME: &str = "all-MiniLM-L6-v2";

/// Longest text, in tokens, the model reads. The rest is ignored.
const MAX_TOKENS: usize = 256;

/// Texts embedded in one pass through the model.
const BATCH: usize = 16;

const MODEL_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

const MODEL_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main";

/// The folder of the model inside the data directory.
pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join(MINILM_NAME)
}

/// Whether every model file is in `dir`.
pub fn model_present(dir: &Path) -> bool {
    MODEL_FILES.iter().all(|f| dir.join(f).is_file())
}

/// Download the model files into `dir`, unless they are there already. Each file is written
/// under a temporary name first, so an interrupted download leaves nothing half-finished.
pub async fn download_model(dir: &Path) -> Result<(), EmbedError> {
    let fail = |e: &dyn std::fmt::Display| EmbedError::Download(e.to_string());
    std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    let client = reqwest::Client::new();
    for file in MODEL_FILES {
        let target = dir.join(file);
        if target.is_file() {
            continue;
        }
        let bytes = client
            .get(format!("{MODEL_URL}/{file}"))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| fail(&e))?
            .bytes()
            .await
            .map_err(|e| fail(&e))?;
        let partial = dir.join(format!("{file}.part"));
        std::fs::write(&partial, &bytes).map_err(|e| fail(&e))?;
        std::fs::rename(&partial, &target).map_err(|e| fail(&e))?;
    }
    Ok(())
}

/// The `all-MiniLM-L6-v2` sentence model on the CPU. Vectors have 384 numbers.
pub struct MiniLm {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl MiniLm {
    /// Load the model from the files in `dir`.
    pub fn load(dir: &Path) -> Result<Self, EmbedError> {
        let fail = |e: &dyn std::fmt::Display| EmbedError::Model(e.to_string());
        let config = std::fs::read_to_string(dir.join("config.json")).map_err(|e| fail(&e))?;
        let config: Config = serde_json::from_str(&config).map_err(|e| fail(&e))?;
        let mut tokenizer =
            Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| fail(&e))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|e| fail(&e))?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        let device = Device::Cpu;
        let weights = std::fs::read(dir.join("model.safetensors")).map_err(|e| fail(&e))?;
        let vb = VarBuilder::from_buffered_safetensors(weights, DType::F32, &device)
            .map_err(|e| fail(&e))?;
        let model = BertModel::load(vb, &config).map_err(|e| fail(&e))?;
        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, candle_core::Error> {
        let run = |e: tokenizers::Error| candle_core::Error::Msg(e.to_string());
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(run)?;
        let column =
            |f: fn(&tokenizers::Encoding) -> &[u32]| -> Result<Tensor, candle_core::Error> {
                let rows = encodings
                    .iter()
                    .map(|e| Tensor::new(f(e), &self.device))
                    .collect::<Result<Vec<_>, _>>()?;
                Tensor::stack(&rows, 0)
            };
        let ids = column(|e| e.get_ids())?;
        let mask = column(|e| e.get_attention_mask())?;
        let types = ids.zeros_like()?;
        let hidden = self.model.forward(&ids, &types, Some(&mask))?;
        // Mean of the token vectors, ignoring padding, then scaled to length one.
        let weights = mask.to_dtype(DType::F32)?.unsqueeze(2)?;
        let summed = hidden.broadcast_mul(&weights)?.sum(1)?;
        let counts = weights.sum(1)?.clamp(1e-9, f64::MAX)?;
        let mean = summed.broadcast_div(&counts)?;
        let norm = mean.sqr()?.sum_keepdim(1)?.sqrt()?.clamp(1e-12, f64::MAX)?;
        mean.broadcast_div(&norm)?.to_vec2::<f32>()
    }
}

impl Embedder for MiniLm {
    fn model(&self) -> &str {
        MINILM_NAME
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(BATCH) {
            out.extend(
                self.embed_batch(chunk)
                    .map_err(|e| EmbedError::Run(e.to_string()))?,
            );
        }
        Ok(out)
    }
}

/// Vectors from hashed words: texts with words in common come out close together. Needs no
/// files, so tests and offline runs can use it. It knows no synonyms.
#[derive(Clone, Copy, Debug, Default)]
pub struct HashEmbedder;

/// Length of the vectors of [`HashEmbedder`].
const HASH_DIMENSIONS: usize = 256;

impl Embedder for HashEmbedder {
    fn model(&self) -> &str {
        "hash-256"
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts
            .iter()
            .map(|text| {
                let mut v = vec![0.0_f32; HASH_DIMENSIONS];
                for word in text
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.chars().count() > 2)
                {
                    let mut hasher = DefaultHasher::new();
                    word.to_lowercase().hash(&mut hasher);
                    v[(hasher.finish() % HASH_DIMENSIONS as u64) as usize] += 1.0;
                }
                crate::vector::normalize(&mut v);
                v
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::cosine;

    fn one(text: &str) -> Vec<f32> {
        HashEmbedder.embed(&[text.to_string()]).unwrap().remove(0)
    }

    #[test]
    fn texts_with_shared_words_are_closer_than_unrelated_ones() {
        let a = one("The lighthouse keeper hides a silver key");
        let b = one("Where is the silver key hidden?");
        let c = one("Dragons sleep beneath the mountain");
        assert!(cosine(&a, &b) > cosine(&a, &c));
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-5, "unit length");
    }

    #[test]
    fn an_empty_text_gives_a_zero_vector_and_the_order_is_kept() {
        let out = HashEmbedder
            .embed(&["".to_string(), "silver key".to_string()])
            .unwrap();
        assert_eq!(out.len(), 2);
        assert!(out[0].iter().all(|x| *x == 0.0));
        assert!(out[1].iter().any(|x| *x > 0.0));
    }

    #[test]
    fn the_model_folder_is_named_after_the_model_and_is_missing_until_downloaded() {
        let dir = std::env::temp_dir().join("masquerole-embed-test");
        let _ = std::fs::remove_dir_all(&dir);
        let model = model_dir(&dir);
        assert!(model.ends_with(MINILM_NAME));
        assert!(!model_present(&model));
        assert!(matches!(MiniLm::load(&model), Err(EmbedError::Model(_))));
        std::fs::create_dir_all(&model).unwrap();
        for f in MODEL_FILES {
            std::fs::write(model.join(f), b"x").unwrap();
        }
        assert!(model_present(&model));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Needs the network and about 90 MB: `cargo test -p masquerole-core -- --ignored minilm`.
    #[tokio::test]
    #[ignore]
    async fn minilm_ranks_by_meaning() {
        let dir = std::env::temp_dir().join("masquerole-minilm-real");
        download_model(&dir).await.unwrap();
        let model = MiniLm::load(&dir).unwrap();
        let v = model
            .embed(&[
                "Where did the old man hide the treasure?".into(),
                "The elder buried his gold under the oak.".into(),
                "It rained all afternoon in the harbor town.".into(),
            ])
            .unwrap();
        assert_eq!(v[0].len(), 384);
        assert!(cosine(&v[0], &v[1]) > cosine(&v[0], &v[2]));
    }
}
