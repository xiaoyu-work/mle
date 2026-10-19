use serde::{Deserialize, Serialize};

use crate::AppResult;

pub const HOST: &str = "api.cognitive.microsofttranslator.com";
pub const PATH: &str = "/translate?api-version=3.0&to=zh-Hans";

#[derive(Serialize)]
struct Input<'a> {
    #[serde(rename = "Text")]
    text: &'a str,
}

#[derive(Deserialize)]
struct Output {
    translations: Vec<Translation>,
}

#[derive(Deserialize)]
struct Translation {
    text: String,
    to: String,
}

pub fn request(text: &str) -> AppResult<Vec<u8>> {
    if text.trim().is_empty() || text.chars().count() > 5000 {
        return Err("Select between 1 and 5000 characters.".into());
    }
    Ok(serde_json::to_vec(&[Input { text }])?)
}

pub fn response(body: &[u8]) -> AppResult<String> {
    let output: Vec<Output> = serde_json::from_slice(body)?;
    output
        .into_iter()
        .flat_map(|item| item.translations)
        .find(|item| item.to == "zh-Hans" && !item.text.trim().is_empty())
        .map(|item| item.text)
        .ok_or_else(|| "The translation service returned no Chinese translation.".into())
}
