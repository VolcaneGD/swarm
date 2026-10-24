ALTER TABLE accounts ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0 CHECK(disabled IN (0,1));
CREATE UNIQUE INDEX single_main_admin ON accounts(is_admin) WHERE is_admin=1;
CREATE TRIGGER protect_main_admin_update BEFORE UPDATE OF disabled,is_admin ON accounts
WHEN OLD.is_admin=1 AND (NEW.disabled!=0 OR NEW.is_admin!=1)
BEGIN SELECT RAISE(ABORT,'main Admin must remain enabled'); END;
CREATE TRIGGER protect_main_admin_delete BEFORE DELETE ON accounts WHEN OLD.is_admin=1
BEGIN SELECT RAISE(ABORT,'main Admin cannot be deleted'); END;
CREATE TABLE roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK(length(trim(name))>0),
    created_at TEXT NOT NULL
);
CREATE TABLE role_members (
    role_id TEXT NOT NULL REFERENCES roles(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    PRIMARY KEY(role_id,account_id)
);
CREATE TABLE account_permissions (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    operation TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('global','board','thread')),
    target_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('allow','deny')),
    CHECK((scope='global' AND target_id='') OR (scope!='global' AND target_id!='')),
    PRIMARY KEY(account_id,operation,scope,target_id)
);
CREATE TABLE role_permissions (
    role_id TEXT NOT NULL REFERENCES roles(id),
    operation TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('global','board','thread')),
    target_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('allow','deny')),
    CHECK((scope='global' AND target_id='') OR (scope!='global' AND target_id!='')),
    PRIMARY KEY(role_id,operation,scope,target_id)
);
