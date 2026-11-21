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
    // 2 — benchmark results (Stage 2). One row per run; a series is one `pco bench run`.
    r#"
    CREATE TABLE benchmarks (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        series           INTEGER NOT NULL,
        target           TEXT    NOT NULL,
        label            TEXT    NOT NULL CHECK (label IN ('before', 'after')),
        created_at       INTEGER NOT NULL,
        frames           INTEGER NOT NULL,
        seconds          REAL    NOT NULL,
        avg_fps          REAL    NOT NULL,
        low1_fps         REAL    NOT NULL,
        avg_frametime_ms REAL    NOT NULL,
        app_version      TEXT    NOT NULL
    );

    CREATE INDEX benchmarks_by_target ON benchmarks (target, label, series);
    "#,
    // 3 — Stage 3 Part C: power plans and preinstalled apps as entry kinds, and one action
    // may now touch several targets (one value per network adapter). SQLite cannot change a
    // CHECK or UNIQUE constraint in place, so the table is rebuilt with the same columns.
    r#"
    CREATE TABLE entries_v3 (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id     INTEGER NOT NULL REFERENCES sessions(id),
        recipe_id      TEXT    NOT NULL,
        recipe_version INTEGER NOT NULL,
        action_index   INTEGER NOT NULL,
        kind           TEXT    NOT NULL CHECK (kind IN ('registry', 'service', 'power', 'appx')),
        target         TEXT    NOT NULL,
        previous_state TEXT    NOT NULL CHECK (json_valid(previous_state)),
        new_state      TEXT    NOT NULL CHECK (json_valid(new_state)),
        status         TEXT    NOT NULL CHECK (status IN ('pending', 'applied', 'rolled_back', 'failed')),
        error          TEXT,
        updated_at     INTEGER NOT NULL,
        UNIQUE (session_id, recipe_id, action_index, target)
    );

    INSERT INTO entries_v3 (id, session_id, recipe_id, recipe_version, action_index, kind,
                            target, previous_state, new_state, status, error, updated_at)
    SELECT id, session_id, recipe_id, recipe_version, action_index, kind,
           target, previous_state, new_state, status, error, updated_at
    FROM entries;

    DROP TABLE entries;
    ALTER TABLE entries_v3 RENAME TO entries;
    CREATE INDEX entries_by_recipe ON entries (recipe_id, status);
    "#,
];
