-- Models
CREATE TABLE drives (
    'id' TEXT NOT NULL,
    'page_token' TEXT NOT NULL,
    PRIMARY KEY('id')
);

CREATE TABLE folders (
    'id' TEXT NOT NULL,
    'drive_id' TEXT NOT NULL,
    'name' TEXT NOT NULL,
    'trashed' BOOLEAN NOT NULL,
    'parent' TEXT,
    PRIMARY KEY('id', 'drive_id'),
    FOREIGN KEY('drive_id') REFERENCES drives('id') ON DELETE CASCADE,
    -- Deferred constraint so integrity is checked at the end of the transaction.
    FOREIGN KEY('parent', 'drive_id') REFERENCES folders('id', 'drive_id') ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE files (
    'id' TEXT NOT NULL,
    'drive_id' TEXT NOT NULL,
    'name' TEXT NOT NULL,
    'trashed' BOOLEAN NOT NULL,
    'parent' TEXT NOT NULL,
    'md5' TEXT NOT NULL,
    'size' BIGINT NOT NULL,
    PRIMARY KEY('id', 'drive_id'),
    FOREIGN KEY('drive_id') REFERENCES drives('id') ON DELETE CASCADE,
    -- Deferred constraint so integrity is checked at the end of the transaction.
    FOREIGN KEY('parent', 'drive_id') REFERENCES folders('id', 'drive_id') ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);

-- Views
CREATE VIEW paths AS
    WITH parents AS (
        -- Initial folders
        SELECT f.id, f.drive_id, f.parent, "/" || f.name as path FROM folders f
        
        UNION ALL
        
        -- Initial files
        SELECT f.id, f.drive_id, f.parent, "/" || f.name as path FROM files f
        
        UNION ALL
        
        -- Recursive clause (using p.id to preserve original id)
        SELECT p.id, f.drive_id, f.parent, "/" || f.name || p.path as path FROM folders f
        INNER JOIN parents p
        WHERE f.id = p.parent AND f.parent IS NOT NULL
    )
    SELECT p.id, p.drive_id, p.path FROM parents p
    WHERE p.drive_id = p.parent;

CREATE VIEW path_changelog ('id', 'drive_id', 'deleted', 'trashed', 'path') AS
    WITH
        changelog_paths AS (
            -- Initial folders
            SELECT f.id, f.drive_id, f.parent, f.deleted, f.trashed, "/" || f.name as path FROM folder_changelog f
            
            UNION ALL
            
            -- Initial files
            SELECT f.id, f.drive_id, f.parent, f.deleted, f.trashed, "/" || f.name as path FROM file_changelog f
            
            UNION ALL
            
            -- Recursive clause (using p.id to preserve original id)
            SELECT p.id, f.drive_id, f.parent, f.deleted, f.trashed, "/" || f.name || p.path as path
            FROM folder_changelog f
            INNER JOIN changelog_paths p ON f.id = p.parent AND f.drive_id = p.drive_id
        ),
        full_paths AS (
            -- Initial changed paths
            SELECT p.id, p.drive_id, p.parent, p.deleted, p.trashed, p.path FROM changelog_paths p
            -- Not exists to only get the "full" path of each id.
            WHERE NOT EXISTS (
                SELECT * FROM changelog_paths p2
                WHERE p2.id = p.parent
            )
            
            UNION ALL
            
            -- Recursive clause
            SELECT p.id, f.drive_id, f.parent, p.deleted, p.trashed, "/" || f.name || p.path as path
            FROM folders f
            INNER JOIN full_paths p ON f.id = p.parent AND f.drive_id = p.drive_id
            WHERE f.parent IS NOT NULL
        )
    SELECT p.id, p.drive_id, p.deleted, p.trashed, p.path FROM full_paths p
    WHERE p.parent = p.drive_id;

-- Changelogs
CREATE TABLE folder_changelog (
    'id' TEXT NOT NULL,
    'drive_id' TEXT NOT NULL,
    'deleted' BOOLEAN NOT NULL,
    'name' TEXT NOT NULL,
    'trashed' BOOLEAN NOT NULL,
    'parent' TEXT,
    PRIMARY KEY('id', 'drive_id', 'deleted')
);

CREATE TABLE file_changelog (
    'id' TEXT NOT NULL,
    'drive_id' TEXT NOT NULL,
    'deleted' BOOLEAN NOT NULL,
    'name' TEXT NOT NULL,
    'trashed' BOOLEAN NOT NULL,
    'parent' TEXT NOT NULL,
    'md5' TEXT NOT NULL,
    'size' BIGINT NOT NULL,
    PRIMARY KEY('id', 'drive_id', 'deleted')
);

-- Folder triggers
CREATE TRIGGER folder_delete
AFTER DELETE ON folders
BEGIN
    INSERT INTO folder_changelog ('id', 'drive_id', 'deleted', 'name', 'trashed', 'parent')
    VALUES (OLD.id, OLD.drive_id, 1, OLD.name, OLD.trashed, OLD.parent);
END;

CREATE TRIGGER folder_update
AFTER UPDATE ON folders
WHEN OLD.name <> NEW.name OR OLD.trashed <> NEW.trashed OR OLD.parent <> NEW.parent
BEGIN
    INSERT INTO folder_changelog ('id', 'drive_id', 'deleted', 'name', 'trashed', 'parent')
    VALUES
        (OLD.id, OLD.drive_id, 1, OLD.name, OLD.trashed, OLD.parent),
        (NEW.id, NEW.drive_id, 0, NEW.name, NEW.trashed, NEW.parent);
END;

CREATE TRIGGER folder_create
AFTER INSERT ON folders
BEGIN
    INSERT INTO folder_changelog ('id', 'drive_id', 'deleted', 'name', 'trashed', 'parent')
    VALUES (NEW.id, NEW.drive_id, 0, NEW.name, NEW.trashed, NEW.parent);
END;

-- File triggers
CREATE TRIGGER file_delete
AFTER DELETE ON files
BEGIN
    INSERT INTO file_changelog ('id', 'drive_id', 'deleted', 'name', 'trashed', 'parent', 'md5', 'size')
    VALUES (OLD.id, OLD.drive_id, 1, OLD.name, OLD.trashed, OLD.parent, OLD.md5, OLD.size);
END;

CREATE TRIGGER file_update
AFTER UPDATE ON files
WHEN OLD.name <> NEW.name OR OLD.trashed <> NEW.trashed OR OLD.parent <> NEW.parent OR OLD.md5 <> NEW.md5 OR OLD.size <> NEW.size
BEGIN
    INSERT INTO file_changelog ('id', 'drive_id', 'deleted', 'name', 'trashed', 'parent', 'md5', 'size')
    VALUES
        (OLD.id, OLD.drive_id, 1, OLD.name, OLD.trashed, OLD.parent, OLD.md5, OLD.size),
        (NEW.id, NEW.drive_id, 0, NEW.name, NEW.trashed, NEW.parent, NEW.md5, NEW.size);
END;

CREATE TRIGGER file_create
AFTER INSERT ON files
BEGIN
    INSERT INTO file_changelog ('id', 'drive_id', 'deleted', 'name', 'trashed', 'parent', 'md5', 'size')
    VALUES (NEW.id, NEW.drive_id, 0, NEW.name, NEW.trashed, NEW.parent, NEW.md5, NEW.size);
END;