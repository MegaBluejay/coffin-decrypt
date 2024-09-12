use std::path::Path;

use anyhow::{anyhow, bail, Result};
use futures::{stream::iter, StreamExt as _};
use glob::glob;
use indicatif::ProgressBar;
use tokio::{
    fs::File,
    io::{copy_buf, AsyncReadExt as _, AsyncWriteExt as _, BufReader, BufWriter},
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let input_paths = glob("**/*.k9a")?.collect::<Result<Vec<_>, _>>()?;

    let bar = ProgressBar::new(input_paths.len() as u64);

    let mut tasks = iter(
        input_paths
            .into_iter()
            .map(|path| async move { (decrypt(&path).await, path) }),
    )
    .buffer_unordered(500);

    while let Some((res, path)) = tasks.next().await {
        if let Err(err) = res {
            eprintln!("{}: {}", path.display(), err);
        }
        bar.inc(1);
    }

    Ok(())
}

async fn decrypt(path: &Path) -> Result<()> {
    let mut input = BufReader::new(File::open(path).await?);

    let ext_len = input.read_u8().await?;

    let mut ext = String::with_capacity(ext_len as usize);
    let read = (&mut input)
        .take(ext_len as u64)
        .read_to_string(&mut ext)
        .await?;
    if read != ext_len as usize {
        bail!("failed to read ext");
    }

    let masked_len = input.read_u8().await?;

    let name = path
        .file_stem()
        .ok_or_else(|| anyhow!("failed to get basename"))?
        .to_str()
        .ok_or_else(|| anyhow!("non-unicode filename"))?;

    if !name.is_ascii() {
        bail!("non-ascii filename");
    }

    let mut mask: u32 = 0;
    for x in name.to_ascii_uppercase().as_bytes() {
        mask = (mask << 1) ^ *x as u32;
    }

    let mut buf = vec![0; masked_len as usize];
    input.read_exact(&mut buf).await?;

    for x in &mut buf {
        let og = *x as u32;
        *x = ((og ^ mask) % 256) as u8;
        mask = (mask << 1) ^ og;
    }

    let mut output = BufWriter::new(File::create(path.with_extension(ext)).await?);

    output.write_all(&buf).await?;
    copy_buf(&mut input, &mut output).await?;

    Ok(())
}
