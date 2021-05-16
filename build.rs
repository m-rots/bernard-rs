use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use std::{env, fs};

#[tokio::main]
async fn main() {
    let out_dir: PathBuf = env::var("OUT_DIR").unwrap().into();
    let db_path = out_dir.join("build.db");

    if db_path.exists() {
        fs::remove_file(&db_path).unwrap();
    }

    let db_url = db_path.to_str().unwrap();

    let pool = SqlitePoolOptions::default()
        .connect(&format!("sqlite:{}?mode=rwc", &db_url))
        .await
        .unwrap();

    sqlx::migrate!().run(&pool).await.unwrap();

    println!("cargo:rustc-env=DATABASE_URL=sqlite:{}", db_url);
}
