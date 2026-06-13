use dotenvy::from_filename;
use std::env;

pub fn load_dotenv_if_exists() {
    match from_filename(".env") {
        Ok(_) => println!(".env loaded"),
        Err(e) => println!(".env load error: {}", e),
    }
}

pub fn print_env_check(key: &str) {
    match env::var(key) {
        Ok(v) => {
            if key == "AWS_SECRET_ACCESS_KEY" || key == "DEEPL_API_KEY" {
                println!("{}=SET(len={})", key, v.len());
            } else {
                println!("{}={}", key, v);
            }
        }
        Err(_) => {
            println!("{}=MISSING", key);
        }
    }
}