pub mod compressor;
pub mod decompressor;

use std::io::Write;

use bitcoinkernel::ChainstateManager;

fn print_progress() {
    if log::log_enabled!(log::Level::Info) {
        print!(".");
        _ = std::io::stdout().flush();
    }
}

struct InputChainstateManager {
    inner: ChainstateManager,
}

impl From<ChainstateManager> for InputChainstateManager {
    fn from(value: ChainstateManager) -> Self {
        Self { inner: value }
    }
}

struct OutputChainstateManager {
    inner: ChainstateManager,
}

impl From<ChainstateManager> for OutputChainstateManager {
    fn from(value: ChainstateManager) -> Self {
        Self { inner: value }
    }
}
