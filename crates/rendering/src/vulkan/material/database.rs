use std::sync::{Mutex, Weak};

use rusqlite::{Connection, OpenFlags};


use engine::filesystem::DIRS;


thread_local! {
    pub static CONN: once_cell::unsync::Lazy<Connection> = once_cell::unsync::Lazy::new(connect);
}

fn connect() -> Connection {
    let path = DIRS.asset.join("materials.db");
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .expect("Failed to open material database")
}

