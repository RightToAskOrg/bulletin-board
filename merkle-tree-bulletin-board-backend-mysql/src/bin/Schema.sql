

create table if not exists PUBLISHED_ROOTS
(
    hash       BINARY(32) PRIMARY KEY NOT NULL,
    prior_hash BINARY(32) NULL,
    timestamp  BIGINT UNSIGNED NOT NULL,
    serial     SERIAL
);

create table if not exists PUBLISHED_ROOT_REFERENCES (
    published    BINARY(32) NOT NULL,
    referenced   BINARY(32) NOT NULL,
    position     INT,
    INDEX (published)
    );


create table if not exists BRANCH (
    hash           BINARY(32) PRIMARY KEY NOT NULL,
    left_child     BINARY(32) UNIQUE NOT NULL,  # left and right are reserved words.
    right_child    BINARY(32) UNIQUE NOT NULL,
    parent         BINARY(32) NULL,
    INDEX (parent)
    );

create table if not exists LEAF (
    hash      BINARY(32) PRIMARY KEY NOT NULL,
    timestamp BIGINT UNSIGNED NOT NULL,
    data      TEXT NULL,
    parent    BINARY(32) NULL,
    INDEX (parent)
    );