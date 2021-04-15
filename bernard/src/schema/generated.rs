table! {
    drives (id) {
        id -> Text,
        page_token -> Text,
    }
}

table! {
    file_changelog (id, drive_id, deleted) {
        id -> Text,
        drive_id -> Text,
        deleted -> Bool,
        name -> Text,
        trashed -> Bool,
        parent -> Text,
        md5 -> Text,
        size -> BigInt,
    }
}

table! {
    files (id, drive_id) {
        id -> Text,
        drive_id -> Text,
        name -> Text,
        trashed -> Bool,
        parent -> Text,
        md5 -> Text,
        size -> BigInt,
    }
}

table! {
    folder_changelog (id, drive_id, deleted) {
        id -> Text,
        drive_id -> Text,
        deleted -> Bool,
        name -> Text,
        trashed -> Bool,
        parent -> Nullable<Text>,
    }
}

table! {
    folders (id, drive_id) {
        id -> Text,
        drive_id -> Text,
        name -> Text,
        trashed -> Bool,
        parent -> Nullable<Text>,
    }
}

joinable!(files -> drives (drive_id));
joinable!(folders -> drives (drive_id));

allow_tables_to_appear_in_same_query!(
    drives,
    file_changelog,
    files,
    folder_changelog,
    folders,
);
