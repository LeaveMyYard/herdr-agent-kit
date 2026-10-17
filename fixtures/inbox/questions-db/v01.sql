BEGIN TRANSACTION;
CREATE TABLE questions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at      TEXT NOT NULL,
    pane_id         TEXT,
    tab_id          TEXT,
    tab_label       TEXT,
    session_id      TEXT,
    cwd             TEXT,
    asked_by        TEXT,
    question        TEXT NOT NULL,
    options         TEXT,
    context         TEXT,
    norm            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'open',
    answer          TEXT,
    answered_at     TEXT,
    delivered_at    TEXT,
    delivery_error  TEXT,
    acknowledged_at TEXT,
    repeat_count    INTEGER NOT NULL DEFAULT 0,
    delivery_attempts INTEGER NOT NULL DEFAULT 0
);
INSERT INTO "questions" VALUES(1,'2026-10-17T15:39:23+00:00','w1:p1',NULL,NULL,'5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69',NULL,'old','Old question one?',NULL,NULL,'old question one','open',NULL,NULL,NULL,NULL,NULL,0,0);
INSERT INTO "questions" VALUES(2,'2026-10-17T15:39:23+00:00','w1:p1',NULL,NULL,'5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69',NULL,NULL,'Old answered?',NULL,NULL,'old answered','answered','yes','2026-10-17T15:39:23+00:00',NULL,NULL,NULL,0,0);
INSERT INTO "questions" VALUES(3,'2026-10-17T15:39:23+00:00',NULL,NULL,NULL,NULL,NULL,NULL,'No pane, no session','["a", "b"]',NULL,'no pane no session','open',NULL,NULL,NULL,NULL,NULL,0,0);
CREATE INDEX questions_status ON questions(status);
CREATE INDEX questions_session ON questions(session_id);
DELETE FROM "sqlite_sequence";
INSERT INTO "sqlite_sequence" VALUES('questions',3);
COMMIT;
