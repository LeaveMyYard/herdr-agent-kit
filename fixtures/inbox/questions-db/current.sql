BEGIN TRANSACTION;
CREATE TABLE alerts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at      TEXT NOT NULL,
    kind            TEXT NOT NULL,
    severity        TEXT NOT NULL,
    title           TEXT NOT NULL,
    message         TEXT,
    pane_id         TEXT,
    tab_id          TEXT,
    tab_label       TEXT,
    session_id      TEXT,
    asked_by        TEXT,
    herdr_server    TEXT,
    status          TEXT NOT NULL DEFAULT 'open',
    acked_at        TEXT,
    closed_at       TEXT,
    reported_at     TEXT
);
CREATE TABLE applied_requests (
    request_id      TEXT PRIMARY KEY,
    qid             INTEGER,
    outcome         TEXT NOT NULL,
    reason          TEXT,
    at              TEXT NOT NULL
);
CREATE TABLE blocked_panes (
    herdr_server    TEXT NOT NULL,
    pane_id         TEXT NOT NULL,
    since           TEXT NOT NULL,
    PRIMARY KEY (herdr_server, pane_id)
);
CREATE TABLE changes (
    seq             INTEGER PRIMARY KEY AUTOINCREMENT,
    qid             INTEGER NOT NULL
);
INSERT INTO "changes" VALUES(1,1);
INSERT INTO "changes" VALUES(2,1);
INSERT INTO "changes" VALUES(3,1);
INSERT INTO "changes" VALUES(4,1);
INSERT INTO "changes" VALUES(5,2);
INSERT INTO "changes" VALUES(6,2);
INSERT INTO "changes" VALUES(7,2);
INSERT INTO "changes" VALUES(8,3);
INSERT INTO "changes" VALUES(9,3);
INSERT INTO "changes" VALUES(10,3);
INSERT INTO "changes" VALUES(11,1);
INSERT INTO "changes" VALUES(12,4);
INSERT INTO "changes" VALUES(13,4);
INSERT INTO "changes" VALUES(14,4);
INSERT INTO "changes" VALUES(15,1);
INSERT INTO "changes" VALUES(16,1);
INSERT INTO "changes" VALUES(17,1);
INSERT INTO "changes" VALUES(18,1);
CREATE TABLE meta (
    key             TEXT PRIMARY KEY,
    value           TEXT
);
INSERT INTO "meta" VALUES('0.5.1 unverified acknowledgements reset','2026-10-17T15:54:00+00:00');
CREATE TABLE question_ext (
    id              INTEGER PRIMARY KEY REFERENCES questions(id),
    header          TEXT,
    multi_select    INTEGER NOT NULL DEFAULT 0,
    answer_json     TEXT,
    group_id        INTEGER,
    draft_json      TEXT,
    drafted_at      TEXT,
    options_json    TEXT,
    hidden_at       TEXT,
    verified_at     TEXT,
    reset_count     INTEGER,
    pending_since   TEXT,
    busy_attempts   INTEGER,
    notified_at     TEXT,
    herdr_server    TEXT,
    agent_gone_at   TEXT,
    legacy_misses   INTEGER,
    legacy_checked_at TEXT
, hook_sent_at TEXT, hook_misses INTEGER);
INSERT INTO "question_ext" VALUES(1,NULL,0,'{"choices": [], "notes": {}, "text": "Postgres"}',1,NULL,NULL,'[{"label": "Postgres", "description": null, "preview": null}, {"label": "SQLite", "description": null, "preview": null}]',NULL,NULL,NULL,NULL,NULL,NULL,'<work>/xdg-config/herdr/herdr.sock',NULL,NULL,NULL,NULL,NULL);
INSERT INTO "question_ext" VALUES(2,NULL,0,NULL,2,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,'<work>/xdg-config/herdr/herdr.sock',NULL,NULL,NULL,NULL,NULL);
INSERT INTO "question_ext" VALUES(3,'Region',0,NULL,2,NULL,NULL,'[{"label": "eu-west-1", "description": "Ireland", "preview": null}, {"label": "us-east-1", "description": null, "preview": null}]',NULL,NULL,NULL,NULL,NULL,NULL,'<work>/xdg-config/herdr/herdr.sock',NULL,NULL,NULL,NULL,NULL);
INSERT INTO "question_ext" VALUES(4,NULL,0,NULL,4,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,'<work>/xdg-config/herdr/herdr.sock',NULL,NULL,NULL,NULL,NULL);
CREATE TABLE question_groups (
    group_id        INTEGER PRIMARY KEY,
    title           TEXT
);
INSERT INTO "question_groups" VALUES(2,'Release');
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
INSERT INTO "questions" VALUES(1,'2026-10-17T15:54:01+00:00','w1:p1','w1:t1','capture','5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69','/capture/cwd','capture','Which database should the service use?','["Postgres", "SQLite"]',NULL,'which database should the service use','answered','Postgres','2026-10-17T15:54:01+00:00',NULL,NULL,'2026-10-17T15:54:01+00:00',2,0);
INSERT INTO "questions" VALUES(2,'2026-10-17T15:54:01+00:00','w1:p1','w1:t1','capture','5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69','/capture/cwd',NULL,'Ship today?',NULL,NULL,'ship today','open',NULL,NULL,NULL,NULL,NULL,0,0);
INSERT INTO "questions" VALUES(3,'2026-10-17T15:54:01+00:00','w1:p1','w1:t1','capture','5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69','/capture/cwd',NULL,'Which region?','["eu-west-1", "us-east-1"]',NULL,'which region','open',NULL,NULL,NULL,NULL,NULL,0,0);
INSERT INTO "questions" VALUES(4,'2026-10-17T15:54:01+00:00','w1:p1','w1:t1','capture','5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69','/capture/cwd',NULL,'Run the migration now?',NULL,NULL,'run the migration now','open',NULL,NULL,NULL,NULL,NULL,0,0);
CREATE TABLE remote_alerts (
    machine_id      TEXT NOT NULL,
    db_id           TEXT NOT NULL,
    alert_id        INTEGER NOT NULL,
    akey            TEXT NOT NULL,
    status          TEXT NOT NULL,
    data_json       TEXT NOT NULL,
    first_seen_at   TEXT NOT NULL,
    PRIMARY KEY (machine_id, alert_id)
);
CREATE TABLE remote_machines (
    machine_id      TEXT PRIMARY KEY,
    label           TEXT,
    target          TEXT,
    db_id           TEXT,
    cursor_seq      INTEGER,
    version         TEXT,
    uv              TEXT,
    herdr           TEXT,
    plugin_root     TEXT,
    pulls           INTEGER NOT NULL DEFAULT 0,
    failures        INTEGER NOT NULL DEFAULT 0,
    last_ok_at      TEXT,
    last_error      TEXT,
    last_try_at     TEXT,
    backoff_until   TEXT
);
CREATE TABLE remote_outbox (
    request_id      TEXT PRIMARY KEY,
    machine_id      TEXT NOT NULL,
    db_id           TEXT NOT NULL,
    remote_id       INTEGER NOT NULL,
    qkey            TEXT NOT NULL,
    action          TEXT NOT NULL,
    detail_json     TEXT,
    created_at      TEXT NOT NULL,
    expires_at      TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'queued',
    result          TEXT,
    sent_at         TEXT,
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_try_at     TEXT,
    sending_at      TEXT
);
CREATE TABLE remote_questions (
    machine_id      TEXT NOT NULL,
    db_id           TEXT NOT NULL,
    remote_id       INTEGER NOT NULL,
    qkey            TEXT NOT NULL,
    status          TEXT NOT NULL,
    data_json       TEXT NOT NULL,
    first_seen_at   TEXT NOT NULL,
    notified_at     TEXT,
    issue_notified_at TEXT,
    cleared_at      TEXT,
    PRIMARY KEY (machine_id, remote_id)
);
CREATE TABLE sessions (
    session_id      TEXT PRIMARY KEY,
    transcript_path TEXT,
    hook_delivery   TEXT,  -- written by 0.9.0 pre-release heads only; unused (columns are never dropped)
    hook_seen_at    TEXT,  -- the PostToolBatch/Stop delivery hooks fired (the WORKING gate)
    watch_seen_at   TEXT,  -- the asyncRewake watcher is waiting (the IDLE gate, with its lock)
    continued_in    TEXT   -- Claude Code continued this conversation in that session (0.10.3)
);
CREATE TABLE tab_decor (
    tab_id          TEXT NOT NULL,
    mark            TEXT NOT NULL,
    PRIMARY KEY (tab_id, mark)
);
CREATE INDEX questions_status ON questions(status);
CREATE INDEX questions_session ON questions(session_id);
CREATE TRIGGER changes_q_ins AFTER INSERT ON questions BEGIN
    INSERT INTO changes (qid) VALUES (NEW.id);
END;
CREATE TRIGGER changes_q_upd AFTER UPDATE ON questions BEGIN
    INSERT INTO changes (qid) VALUES (NEW.id);
END;
CREATE TRIGGER changes_q_del AFTER DELETE ON questions BEGIN
    INSERT INTO changes (qid) VALUES (OLD.id);
END;
CREATE TRIGGER changes_x_ins AFTER INSERT ON question_ext BEGIN
    INSERT INTO changes (qid) VALUES (NEW.id);
END;
CREATE TRIGGER changes_x_upd AFTER UPDATE ON question_ext BEGIN
    INSERT INTO changes (qid) VALUES (NEW.id);
END;
CREATE INDEX alerts_status ON alerts(status);
CREATE VIEW questions_v6 AS
    SELECT q.id, q.created_at, q.pane_id, q.tab_id, q.tab_label, q.session_id, q.cwd, q.asked_by,
           q.question, q.options, q.context, q.norm, q.status, q.answer, q.answered_at, q.delivered_at,
           q.delivery_error, q.acknowledged_at, q.repeat_count, q.delivery_attempts,
           e.header, COALESCE(e.multi_select, 0) AS multi_select, e.answer_json,
           COALESCE(e.group_id, q.id) AS group_id, e.draft_json, e.drafted_at, e.options_json, e.hidden_at,
           g.title AS group_title, e.verified_at
    FROM questions q LEFT JOIN question_ext e ON e.id = q.id
    LEFT JOIN question_groups g ON g.group_id = COALESCE(e.group_id, q.id);
DELETE FROM "sqlite_sequence";
INSERT INTO "sqlite_sequence" VALUES('changes',18);
INSERT INTO "sqlite_sequence" VALUES('questions',4);
COMMIT;
