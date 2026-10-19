use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::{Path, PathBuf};

use crate::AppResult;

pub struct Dictionary(Connection);

pub struct Sense<'a> {
    pub domain: Option<&'a str>,
    pub meaning: &'a str,
}

pub fn senses(meaning: &str) -> Vec<Sense<'_>> {
    let mut senses = Vec::new();
    for line in meaning
        .split(['\n', ';', '\u{ff1b}'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (domain, meaning) = match line.strip_prefix('[').and_then(|line| line.split_once(']')) {
            Some((domain, meaning)) if !meaning.trim().is_empty() => (Some(domain), meaning.trim()),
            _ => (None, line),
        };
        if !senses
            .iter()
            .any(|sense: &Sense<'_>| sense.domain == domain && sense.meaning == meaning)
        {
            senses.push(Sense { domain, meaning });
        }
    }
    senses
}

impl Dictionary {
    pub fn open(path: &Path) -> AppResult<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.execute_batch("PRAGMA cache_size = -256; PRAGMA mmap_size = 0;")?;
        connection.prepare("SELECT meaning FROM words WHERE word = ?1")?;
        Ok(Self(connection))
    }

    pub fn lookup(&self, text: &str) -> AppResult<Option<String>> {
        let word = text.trim().to_ascii_lowercase();
        if word.len() > 80
            || word.is_empty()
            || !word
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'\'' || b == b'-')
        {
            return Ok(None);
        }
        Ok(self
            .0
            .query_row(
                "SELECT meaning FROM words WHERE word = ?1",
                [&word],
                |row| row.get(0),
            )
            .optional()?)
    }
}

pub fn default_path() -> AppResult<PathBuf> {
    Ok(std::env::current_exe()?
        .parent()
        .ok_or("Cannot locate application directory.")?
        .join("data")
        .join("dictionary.db"))
}
