use bernard::{auth::Account, Bernard, ChangedFile, ChangedFolder};
use clap::{App, Arg};
use colored::*;
use shadow_rs::shadow;
use std::path::PathBuf;

shadow!(build);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = App::new("Bernard")
        .version(build::clap_version().as_str())
        .author("Storm Timmermans (@m-rots)")
        .arg(
            Arg::new("account")
                .long("account")
                .alias("sa")
                .alias("service-account")
                .short('a')
                .takes_value(true)
                .value_name("FILE")
                .default_value("account.json")
                .about("Path of the Service Account JSON key")
                .required(true),
        )
        .arg(
            Arg::new("database")
                .long("database")
                .alias("db")
                .takes_value(true)
                .value_name("FILE")
                .about("Path of the sqlite3 database file")
                .default_value("bernard-testing.db"),
        )
        .arg(
            Arg::new("drive_id")
                .index(1)
                .required(true)
                .takes_value(true)
                .value_name("DRIVE_ID")
                .about("The ID of the Shared Drive"),
        )
        .arg(
            Arg::new("proxy")
                .long("proxy")
                .short('p')
                .takes_value(true)
                .value_name("URL")
                .about("Proxy URL to use for debugging"),
        )
        .subcommand(App::new("reset").about("Combination of remove + init"))
        .subcommand(App::new("remove").about("Remove a Shared Drive from the database"))
        .subcommand(App::new("add").about("Add Shared Drive (init + fill)"))
        .get_matches();

    // Defaults to "bernard-testing.db" so can unwrap
    let database_path = matches.value_of("database").unwrap();

    // Required value so can unwrap
    let drive_id = matches.value_of("drive_id").unwrap();

    // Required value so can unwrap
    let account_file_name = matches.value_of("account").unwrap();
    let account = Account::from_file(account_file_name);

    let mut bernard = Bernard::builder(database_path, &account);

    if let Some(proxy) = matches.value_of("proxy") {
        bernard = bernard.proxy(proxy);
    }

    let mut bernard = bernard.build().await?;

    match matches.subcommand() {
        Some(("reset", _)) => {
            bernard.remove_drive(drive_id)?;
            bernard.add_drive(drive_id).await?;
        }
        Some(("remove", _)) => {
            bernard.remove_drive(drive_id)?;
        }
        Some(("add", _)) => {
            bernard.add_drive(drive_id).await?;
        }
        None => {
            bernard.sync_drive(drive_id).await?;

            let now = std::time::Instant::now();
            let folders = bernard.get_changed_folders_paths(drive_id)?;
            let files = bernard.get_changed_files_paths(drive_id)?;

            println!(
                "Running the changelog-based queries took {:?}",
                now.elapsed()
            );
            list_changes(folders, files);
        }
        _ => (),
    }

    Ok(())
}

fn format_path(id: &str, path: &std::path::Path, trashed: bool) -> String {
    match trashed {
        true => format!("{} {:?} {}", id.dimmed(), path, "(trashed)".bright_red()),
        false => format!("{} {:?}", id.dimmed(), path),
    }
}

fn list_changes<I, J>(folders: I, files: J)
where
    I: IntoIterator<Item = (ChangedFolder, PathBuf)>,
    J: IntoIterator<Item = (ChangedFile, PathBuf)>,
{
    for (folder, path) in folders {
        match folder {
            ChangedFolder::Created(folder) => {
                println!(
                    "{}: {}",
                    "Created".bright_green(),
                    format_path(&folder.id, &path, folder.trashed)
                )
            }
            ChangedFolder::Deleted(folder) => {
                println!(
                    "{}: {}",
                    "Deleted".bright_red(),
                    format_path(&folder.id, &path, folder.trashed)
                )
            }
        }
    }

    for (file, path) in files {
        match file {
            ChangedFile::Created(file) => {
                println!(
                    "{}: {}",
                    "Created".bright_green(),
                    format_path(&file.id, &path, file.trashed)
                )
            }
            ChangedFile::Deleted(file) => {
                println!(
                    "{}: {}",
                    "Deleted".bright_red(),
                    format_path(&file.id, &path, file.trashed)
                )
            }
        }
    }
}
