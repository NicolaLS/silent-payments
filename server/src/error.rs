use derive_more::From;

#[derive(Debug, From)]
pub enum Error {
    Config,
    InvalidInput,

    // -- module server.rs
    // FIXME: Should belong to DB but right now handlers decide whether it was found or not..
    NotFound,

    // -- module: sync.rs
    #[from]
    BitcoinRpc(bitcoincore_rpc::Error),
    #[from]
    SendBlock(tokio::sync::mpsc::error::SendError<crate::SPBlock>),

    // -- module: store.rs
    #[from]
    Db(sqlx::Error),

    // -- external
    #[from]
    Io(std::io::Error),
}

pub type Result<T> = core::result::Result<T, Error>;

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}
