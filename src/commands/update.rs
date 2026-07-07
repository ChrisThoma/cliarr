use std::time::Duration;

use serde::Serialize;

use crate::api::http;
use crate::commands::output;
use crate::error::{CliarrError, Result};
use crate::outln;
use crate::update::{
    self, CHECKSUMS_ASSET, CURRENT_VERSION, GITHUB_API, Release, TARGET,
};

#[derive(Serialize)]
struct CheckOutput<'a> {
    current: &'a str,
    latest: &'a str,
    update_available: bool,
}

pub async fn run(check_only: bool, json: bool) -> Result<()> {
    let release = update::fetch_latest(&http::build_client(), GITHUB_API).await?;
    // Keep the passive daily notice in sync with what we just learned.
    update::write_cache(release.version());
    let newer = update::is_newer(CURRENT_VERSION, &release.tag_name);

    if check_only {
        if json {
            return output::print_json(&CheckOutput {
                current: CURRENT_VERSION,
                latest: release.version(),
                update_available: newer.is_some(),
            });
        }
        match &newer {
            Some(latest) => outln!("cliarr v{latest} is available (you have v{CURRENT_VERSION}); run `cliarr update`"),
            None => outln!("cliarr v{CURRENT_VERSION} is up to date"),
        }
        return Ok(());
    }

    let Some(latest) = newer else {
        outln!("cliarr v{CURRENT_VERSION} is up to date");
        return Ok(());
    };

    install(&release).await?;
    outln!("updated cliarr v{CURRENT_VERSION} → v{latest}");
    Ok(())
}

async fn install(release: &Release) -> Result<()> {
    let name = update::asset_name(TARGET);
    let asset = release.asset(&name).ok_or_else(|| {
        CliarrError::Other(format!(
            "release {} has no prebuilt binary for {TARGET} (asset {name}); \
             update with `cargo install --git https://github.com/{}`",
            release.tag_name,
            update::REPO
        ))
    })?;
    let sums = release.asset(CHECKSUMS_ASSET).ok_or_else(|| {
        CliarrError::Other(format!("release {} has no {CHECKSUMS_ASSET} asset", release.tag_name))
    })?;

    // The shared client's 15s total timeout is too short for a multi-MB
    // download; keep only the connect timeout here.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("cliarr/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| CliarrError::Other(format!("cannot build HTTP client: {e}")))?;

    eprintln!("downloading {name} …");
    let sums_text = http::check("GitHub", client.get(&sums.browser_download_url).send().await?)
        .await?
        .text()
        .await?;
    let expected = update::checksum_for(&sums_text, &name).ok_or_else(|| {
        CliarrError::Other(format!("{CHECKSUMS_ASSET} has no entry for {name}"))
    })?;
    let bytes = http::check("GitHub", client.get(&asset.browser_download_url).send().await?)
        .await?
        .bytes()
        .await?;
    update::verify_checksum(&bytes, &expected)?;

    // Stage next to the running executable: same filesystem, so the final
    // swap is an atomic rename.
    let exe = std::env::current_exe()
        .map_err(|e| CliarrError::Other(format!("cannot locate the running executable: {e}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| CliarrError::Other("the running executable has no parent directory".into()))?;
    let staged = dir.join(format!(".cliarr-update.{}", std::process::id()));
    let write_err = |e: std::io::Error| {
        CliarrError::Other(format!("cannot write {}: {e}", staged.display()))
    };
    std::fs::write(&staged, &bytes).map_err(write_err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .map_err(write_err)?;
    }

    let result = self_replace::self_replace(&staged)
        .map_err(|e| CliarrError::Other(format!("cannot replace {}: {e}", exe.display())));
    let _ = std::fs::remove_file(&staged);
    result
}
