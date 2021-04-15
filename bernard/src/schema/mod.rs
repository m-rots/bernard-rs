mod generated;

pub use generated::*;

// Create custom tables for views
table! {
    paths (id, drive_id) {
        id -> Text,
        drive_id -> Text,
        path -> Text,
    }
}

table! {
    path_changelog (id, drive_id, deleted) {
        id -> Text,
        drive_id -> Text,
        deleted -> Bool,
        trashed -> Bool,
        path -> Text,
    }
}
