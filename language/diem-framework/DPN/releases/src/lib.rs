// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use diem_types::transaction::ScriptFunction;
use include_dir::{include_dir, Dir};
use move_binary_format::file_format::CompiledModule;
use move_command_line_common::files::{extension_equals, find_filenames, MOVE_COMPILED_EXTENSION};
use once_cell::sync::Lazy;
use std::{convert::TryFrom, path::PathBuf};

pub mod legacy;

#[cfg(test)]
mod tests;

/// The compiled library needs to be included in the Rust binary due to Docker deployment issues.
const RELEASES_DIR: Dir = include_dir!("artifacts");

const NON_PACKAGE_NAMES: &[&str] = &["release-1.2.0-rc0", "release-1.4.0-rc0"];

/// Return a list of all available releases.
pub fn list_all_releases() -> Result<Vec<String>> {
    Ok(RELEASES_DIR
        .dirs()
        .iter()
        .map(|dir| {
            dir.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect())
}

/// Load the serialized modules from the specified release.
pub fn load_modules_from_release(release_name: &str) -> Result<Vec<Vec<u8>>> {
    if !NON_PACKAGE_NAMES.contains(&release_name) {
        diem_framework::releases::ReleaseFetcher::new("DPN", release_name).module_blobs()
    } else {
        let mut modules_path = PathBuf::from(release_name);
        modules_path.push("modules");
        match RELEASES_DIR.get_dir(&modules_path) {
            Some(modules_dir) => {
                let mut modules = modules_dir
                    .files()
                    .iter()
                    .filter(|file| extension_equals(file.path(), MOVE_COMPILED_EXTENSION))
                    .map(|file| (file.path().file_name(), file.contents().to_vec()))
                    .collect::<Vec<_>>();

                modules.sort_by(|(name1, _), (name2, _)| name1.cmp(name2));

                Ok(modules.into_iter().map(|(_name, blob)| blob).collect())
            }
            None => bail!("release {} not found", release_name),
        }
    }
}

/// Load the error descriptions from the specified release.
pub fn load_error_descriptions_from_release(release_name: &str) -> Result<Vec<u8>> {
    diem_framework::releases::ReleaseFetcher::new("DPN", release_name).error_descriptions()
}

/// Load the serialized modules from the specified paths.
pub fn load_modules_from_paths(paths: &[PathBuf]) -> Vec<Vec<u8>> {
    find_filenames(paths, |path| {
        extension_equals(path, MOVE_COMPILED_EXTENSION)
    })
    .expect("module loading failed")
    .iter()
    .map(|file_name| std::fs::read(file_name).unwrap())
    .collect::<Vec<_>>()
}

static CURRENT_MODULE_BLOBS: Lazy<Vec<Vec<u8>>> =
    Lazy::new(|| load_modules_from_release("current").unwrap());

static CURRENT_MODULES: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
    CURRENT_MODULE_BLOBS
        .iter()
        .map(|blob| CompiledModule::deserialize(blob).unwrap())
        .collect()
});

pub fn current_modules() -> &'static [CompiledModule] {
    &CURRENT_MODULES
}

pub fn current_module_blobs() -> &'static [Vec<u8>] {
    &CURRENT_MODULE_BLOBS
}

pub fn current_modules_with_blobs(
) -> impl Iterator<Item = (&'static Vec<u8>, &'static CompiledModule)> {
    CURRENT_MODULE_BLOBS.iter().zip(CURRENT_MODULES.iter())
}

static CURRENT_ERROR_DESCRIPTIONS: Lazy<Vec<u8>> =
    Lazy::new(|| load_error_descriptions_from_release("current").unwrap());

pub fn current_error_descriptions() -> &'static [u8] {
    &CURRENT_ERROR_DESCRIPTIONS
}

pub fn name_for_script(bytes: &[u8]) -> Result<String> {
    if let Ok(script) = legacy::transaction_scripts::LegacyStdlibScript::try_from(bytes) {
        Ok(format!("{}", script))
    } else {
        bcs::from_bytes::<ScriptFunction>(bytes)
            .map(|script| {
                format!(
                    "{}::{}::{}",
                    script.module().address().short_str_lossless(),
                    script.module().name(),
                    script.function()
                )
            })
            .map_err(|err| err.into())
    }
}
