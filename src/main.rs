#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod dictionary;
mod translation;
#[cfg(windows)]
mod windows_app;

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() {
    if let Err(error) = run() {
        #[cfg(windows)]
        windows_app::show_error(&error.to_string());
        #[cfg(not(windows))]
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> AppResult<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--lookup") {
        let word = args
            .get(2)
            .ok_or("Usage: mle --lookup WORD [OUTPUT_FILE]")?;
        let dictionary = dictionary::Dictionary::open(&dictionary::default_path()?)?;
        let meaning = dictionary.lookup(word)?.ok_or("Word not found")?;
        if let Some(path) = args.get(3) {
            std::fs::write(path, meaning)?;
        } else {
            println!("{meaning}");
        }
        return Ok(());
    }
    #[cfg(windows)]
    return windows_app::run();
    #[cfg(not(windows))]
    Err("The shared dictionary core is portable; the desktop integration currently supports Windows only.".into())
}
