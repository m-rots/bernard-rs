-- Adding the `trashed` column to the `path changelog` view.
DROP VIEW path_changelog;

CREATE VIEW path_changelog AS
    WITH
        changelog_paths AS (
            -- Initial folders
            SELECT 1 as 'folder', f.id, f.drive_id, f.parent, f.deleted, f.trashed, "/" || f.name as path FROM folder_changelog f
            
            UNION ALL
            
            -- Initial files
            SELECT 0 as 'folder', f.id, f.drive_id, f.parent, f.deleted, f.trashed, "/" || f.name as path FROM file_changelog f
            
            UNION ALL
            
            -- Recursive clause (using p.id to preserve original id)
            SELECT p.folder, p.id, f.drive_id, f.parent, f.deleted, f.trashed, "/" || f.name || p.path as path
            FROM folder_changelog f, changelog_paths p
            WHERE f.id = p.parent AND f.drive_id = p.drive_id
        ),
        full_paths AS (
            -- Initial changed paths
            SELECT p.folder, p.id, p.drive_id, p.parent, p.deleted, p.trashed, p.path FROM changelog_paths p
            -- Not exists to only get the "full" path of each id.
            WHERE NOT EXISTS (
                SELECT * FROM changelog_paths p2
                WHERE p2.id = p.parent
            )
            
            UNION ALL
            
            -- Recursive clause
            SELECT p.folder, p.id, f.drive_id, f.parent, p.deleted, p.trashed, "/" || f.name || p.path as path
            FROM folders f
            INNER JOIN full_paths p ON f.id = p.parent AND f.drive_id = p.drive_id
            WHERE f.parent IS NOT NULL
        )
    SELECT p.folder, p.id, p.drive_id, p.deleted, p.trashed, p.path FROM full_paths p
    WHERE p.parent = p.drive_id;