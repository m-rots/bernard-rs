Bernard
=======

Bernard aims to be a _correct\*_ synchronisation engine for [Google Drive](https://www.google.com/drive/) metadata.
In particular, Bernard,

- Stores file and folder metadata in a [SQLite](https://www.sqlite.org/index.html) database.
- Keeps track of the changes made in the previous synchronisation.

_\*The metadata in the database should be a one-to-one copy of the current state within Google Drive after a synchronisation._

## Example

This example uses [Tokio](https://tokio.rs), so your `Cargo.toml` should look like this:

```toml
[dependencies]
bernard = { git = "https://github.com/m-rots/bernard-rs" }
tokio = { version = "1", features = ["full"] }
```

And then the code:

```rust + no_run
use bernard::{Account, Bernard, SyncKind};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the Service Account JSON Key from a file.
    let account = Account::from_file("account.json")?;

    // Build Bernard.
    let bernard = Bernard::builder("bernard.db", account).build().await?;

    // Sync the provided Shared Drive.
    // Replace the drive_id with a Shared Drive ID your service account has access to.
    match bernard.sync_drive("0A1xxxxxxxxxUk9PVA").await? {
        // Do not do anything on a full-sync.
        SyncKind::Full => (),

        // Print the changes this partial sync fetched.
        SyncKind::Partial(changes) => {
            let paths = changes.paths().await?;
            println!("changed paths: {:#?}", paths);
        }
    }

    // Close Bernard's internal connection pool.
    // This is required to clean up the .wal and .shm files on shutdown.
    bernard.close().await;

    Ok(())
}
```

## Overview

To use Bernard, you must create a [Service Account](https://cloud.google.com/iam/docs/service-accounts) and then invite this account (the email address) to one or multiple [Shared Drives](https://support.google.com/a/answer/7212025) that you want Bernard to have access to.
The Service Account should at least have `Reader` permission.
Last but not least, do not forget to [enable the Google Drive API](https://developers.google.com/drive/api/v3/enable-drive-api) in the Google Cloud Project you created the Service Account in.

## Limitations

- Bernard does not work with _My Drive_ nor _Shared with me_.
- Bernard's only authentication method is through [Service Accounts](https://cloud.google.com/iam/docs/service-accounts).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.