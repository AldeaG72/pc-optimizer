//! Versioned schema. `PRAGMA user_version` holds the number of migrations applied.
//!
//! Never edit a migration that has shipped; add a new one at the end.

pub(crate) const MIGRATIONS: &[&str] = &[
    // 1 — sessions and write-ahead entries.
    r#"
    CREATE TABLE sessions (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        kind        TEXT    NOT NULL CHECK (kind IN ('apply', 'undo')),
        status      TEXT    NOT NULL CHECK (status IN ('pending', 'applied', 'rolled_back', 'failed')),
        started_at  INTEGER NOT NULL,
        finished_at INTEGER,
        app_version TEXT    NOT NULL
    );

    CREATE TABLE entries (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id     INTEGER NOT NULL REFERENCES sessions(id),
        recipe_id      TEXT    NOT NULL,
        recipe_version INTEGER NOT NULL,
        action_index   INTEGER NOT NULL,
        kind           TEXT    NOT NULL CHECK (kind IN ('registry', 'service')),
        target         TEXT    NOT NULL,
        previous_state TEXT    NOT NULL CHECK (json_valid(previous_state)),
        new_state      TEXT    NOT NULL CHECK (json_valid(new_state)),
        status         TEXT    NOT NULL CHECK (status IN ('pending', 'applied', 'rolled_back', 'failed')),
        error          TEXT,
        updated_at     INTEGER NOT NULL,
        UNIQUE (session_id, recipe_id, action_index)
    );

    CREATE INDEX entries_by_recipe ON entries (recipe_id, status);
    "#,
];
