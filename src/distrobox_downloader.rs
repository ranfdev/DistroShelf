use crate::distrobox_task::DistroboxTask;
use crate::fakers::Command;
use crate::fakers::CommandRunner;
use crate::store::root_store::RootStore;
use anyhow::{Context, anyhow};
use gtk::glib;
use gtk::prelude::*;
use std::path::PathBuf;

pub const DISTROBOX_VERSION: &str = "1.8.2.1";
// SHA256 of the tar.gz file from github
pub const DISTROBOX_SHA256: &str =
    "2c6b2ac9e0db04eb22edab1572b1e62284f5f651c292f536c59fb908d573d0a2";

pub fn get_bundled_distrobox_path() -> PathBuf {
    let user_data_dir = glib::user_data_dir();
    user_data_dir
        .join("distroshelf")
        .join(format!("distrobox-{}", DISTROBOX_VERSION))
        .join("distrobox")
}

pub fn get_bundled_distrobox_dir() -> PathBuf {
    let user_data_dir = glib::user_data_dir();
    user_data_dir.join("distroshelf")
}

fn log(task: &DistroboxTask, msg: &str) {
    task.output().insert(&mut task.output().end_iter(), msg);
    task.output().insert(&mut task.output().end_iter(), "\n");
}

pub fn download_distrobox(root_store: &RootStore) -> DistroboxTask {
    let root_store_weak = root_store.downgrade();
    // We should be able to actually use a CommandRunner runs in the flatpak sandbox, because it has all the tools we need.
    // Also, the data folder is writable there and should be mapped to the host.
    let command_runner = CommandRunner::new_real();

    DistroboxTask::new("system", "Downloading Distrobox", move |task| async move {
        let download_dir = get_bundled_distrobox_dir();
        let tarball_path = download_dir.join("distrobox.tar.gz");
        let url = format!(
            "https://github.com/89luca89/distrobox/archive/refs/tags/{}.tar.gz",
            DISTROBOX_VERSION
        );

        // Ensure directory exists
        std::fs::create_dir_all(&download_dir).context("Failed to create download directory")?;

        log(
            &task,
            &format!("Using download directory: {:?}", download_dir),
        );

        // 1. Download
        log(&task, &format!("Downloading {}...", url));
        let mut curl_cmd = Command::new("curl");
        curl_cmd.arg("-L");
        curl_cmd.arg("-o");
        curl_cmd.arg(tarball_path.to_str().unwrap());
        curl_cmd.arg(&url);
        curl_cmd.stdout = crate::fakers::FdMode::Pipe;
        curl_cmd.stderr = crate::fakers::FdMode::Pipe;

        let child = command_runner
            .spawn(curl_cmd)
            .context("Failed to run curl")?;

        task.handle_child_output(child).await?;

        // 2. Verify SHA256
        log(&task, "Verifying checksum...");
        let mut sha_cmd = Command::new("sha256sum");
        sha_cmd.arg(tarball_path.to_str().unwrap());
        sha_cmd.stdout = crate::fakers::FdMode::Pipe;
        sha_cmd.stderr = crate::fakers::FdMode::Pipe;

        let output = command_runner.output(sha_cmd).await?;
        if !output.status.success() {
            return Err(anyhow!("sha256sum failed"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let calculated_hash = stdout.split_whitespace().next().unwrap_or("");

        if calculated_hash != DISTROBOX_SHA256 {
            return Err(anyhow!(
                "Checksum mismatch. Expected {}, got {}",
                DISTROBOX_SHA256,
                calculated_hash
            ));
        }
        log(&task, "Checksum verified.");

        // 3. Extract
        log(&task, "Extracting...");
        let mut tar_cmd = Command::new("tar");
        tar_cmd.arg("xzf");
        tar_cmd.arg(tarball_path.to_str().unwrap());
        tar_cmd.arg("-C");
        tar_cmd.arg(download_dir.to_str().unwrap());
        tar_cmd.stdout = crate::fakers::FdMode::Pipe;
        tar_cmd.stderr = crate::fakers::FdMode::Pipe;

        let child = command_runner.spawn(tar_cmd).context("Failed to run tar")?;

        task.handle_child_output(child).await?;

        // 3b. Clean up tarball
        log(&task, "Removing tarball...");
        std::fs::remove_file(&tarball_path).context("Failed to remove tarball")?;

        // 4. Make executable (it should be already, but just in case)
        let binary_path = get_bundled_distrobox_path();
        log(
            &task,
            &format!("Setting executable permissions on {:?}...", binary_path),
        );

        let mut chmod_cmd = Command::new("chmod");
        chmod_cmd.arg("+x");
        chmod_cmd.arg(binary_path.to_str().unwrap());
        chmod_cmd.stdout = crate::fakers::FdMode::Pipe;
        chmod_cmd.stderr = crate::fakers::FdMode::Pipe;

        let output = command_runner.output(chmod_cmd).await?;
        if !output.status.success() {
            return Err(anyhow!("chmod failed"));
        }

        log(&task, "Distrobox installed successfully.");

        if let Some(root_store) = root_store_weak.upgrade() {
            root_store.distrobox_version().refetch();
            root_store.set_current_dialog(crate::tagged_object::TaggedObject::default());
        }

        Ok(())
    })
}
