//! Build a small, disposable boot image from a user-supplied trusted static BusyBox.
//! The output is a Linux "newc" initramfs, not a general-purpose image builder.

use spacetime_types::{Result, SpacetimeError};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

const MAX_BUSYBOX: u64 = 32 * 1024 * 1024;
const INIT: &str = "#!/bin/sh\n\
/bin/busybox mount -t proc proc /proc\n\
/bin/busybox mount -t sysfs sysfs /sys\n\
/bin/busybox mount -t devtmpfs devtmpfs /dev\n\
echo 'Spacetime experimental guest: trusted software only.'\n\
echo 'Type exit to shut down. Changes are not saved.'\n\
/bin/sh\n\
/bin/busybox poweroff -f\n";
const APPLETS: &[&str] = &[
    "sh", "ls", "cat", "echo", "pwd", "mkdir", "rm", "cp", "mv", "touch", "uname", "ps", "mount",
    "poweroff", "dmesg", "grep", "find", "sleep", "vi",
];

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf)> {
    let usage = "usage: space initramfs --busybox <static-binary> --out <new-file>";
    let mut busybox = None;
    let mut out = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let slot = match arg.as_str() {
            "--busybox" => &mut busybox,
            "--out" => &mut out,
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unexpected argument: {arg}\n{usage}"
                )))
            }
        };
        let value = iter
            .next()
            .filter(|value| !value.starts_with('-'))
            .ok_or_else(|| SpacetimeError::new(format!("missing value for {arg}\n{usage}")))?;
        if slot.replace(PathBuf::from(value)).is_some() {
            return Err(SpacetimeError::new(format!("duplicate {arg}")));
        }
    }
    Ok((
        busybox.ok_or_else(|| SpacetimeError::new(format!("missing --busybox\n{usage}")))?,
        out.ok_or_else(|| SpacetimeError::new(format!("missing --out\n{usage}")))?,
    ))
}

// The ELF interpreter segment indicates a dynamically linked binary, which cannot start in
// our image because it contains no dynamic loader or libraries. This is an integrity check,
// not a trust or security check of the supplied executable.
fn validate_busybox(data: &[u8]) -> Result<()> {
    let invalid = || SpacetimeError::new("BusyBox must be a static x86-64 Linux ELF executable");
    if data.len() < 64
        || &data[..6] != b"\x7fELF\x02\x01"
        || u16::from_le_bytes([data[18], data[19]]) != 62
    {
        return Err(invalid());
    }
    let offset = u64::from_le_bytes(data[32..40].try_into().map_err(|_| invalid())?);
    let size = u16::from_le_bytes([data[54], data[55]]) as u64;
    let count = u16::from_le_bytes([data[56], data[57]]) as u64;
    if size < 56 || count == 0 {
        return Err(invalid());
    }
    let end = offset
        .checked_add(size.checked_mul(count).ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    if end > data.len() as u64 {
        return Err(invalid());
    }
    for index in 0..count {
        let pos = (offset + index * size) as usize;
        let kind = u32::from_le_bytes(data[pos..pos + 4].try_into().map_err(|_| invalid())?);
        if kind == 3 {
            return Err(invalid());
        }
    }
    Ok(())
}

// Archive entries have deterministic metadata and 4-byte-aligned headers, names and contents.
fn entry(archive: &mut Vec<u8>, name: &str, mode: u32, contents: &[u8], rdev: (u32, u32)) {
    let name_len = name.len() + 1;
    let fields = [
        0,
        mode,
        0,
        0,
        1,
        0,
        contents.len() as u32,
        0,
        0,
        rdev.0,
        rdev.1,
        name_len as u32,
        0,
    ];
    archive.extend_from_slice(b"070701");
    for field in fields {
        archive.extend_from_slice(format!("{field:08x}").as_bytes());
    }
    archive.extend_from_slice(name.as_bytes());
    archive.push(0);
    while archive.len() % 4 != 0 {
        archive.push(0);
    }
    archive.extend_from_slice(contents);
    while archive.len() % 4 != 0 {
        archive.push(0);
    }
}

fn build_archive(busybox: &[u8]) -> Result<Vec<u8>> {
    validate_busybox(busybox)?;
    let mut archive = Vec::new();
    for dir in ["bin", "dev", "proc", "sys", "tmp"] {
        entry(&mut archive, dir, 0o040755, &[], (0, 0));
    }
    entry(&mut archive, "dev/console", 0o020600, &[], (5, 1));
    entry(&mut archive, "dev/null", 0o020666, &[], (1, 3));
    entry(&mut archive, "bin/busybox", 0o100755, busybox, (0, 0));
    for applet in APPLETS {
        entry(
            &mut archive,
            &format!("bin/{applet}"),
            0o120777,
            b"busybox",
            (0, 0),
        );
    }
    entry(&mut archive, "init", 0o100755, INIT.as_bytes(), (0, 0));
    entry(&mut archive, "TRAILER!!!", 0, &[], (0, 0));
    Ok(archive)
}

pub fn cmd_initramfs(args: &[String]) -> Result<()> {
    let (input, output) = parse_args(args)?;
    let source = File::open(&input)?;
    let metadata = source.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BUSYBOX {
        return Err(SpacetimeError::new(
            "BusyBox must be a regular file no larger than 32 MiB",
        ));
    }
    let mut data = Vec::new();
    source.take(MAX_BUSYBOX + 1).read_to_end(&mut data)?;
    // Limit the read even if the input grows after the metadata check.
    if data.len() as u64 > MAX_BUSYBOX {
        return Err(SpacetimeError::new("BusyBox is larger than 32 MiB"));
    }
    let archive = build_archive(&data)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    if let Err(error) = file.write_all(&archive).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&output);
        return Err(error.into());
    }
    println!("Created experimental initramfs: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elf() -> Vec<u8> {
        let mut bytes = vec![0; 120];
        bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[64..68].copy_from_slice(&1u32.to_le_bytes());
        bytes
    }

    #[test]
    fn rejects_dynamic_wrong_arch_and_truncated_images() {
        let mut binary = elf();
        assert!(validate_busybox(&binary).is_ok());
        binary[64] = 3;
        assert!(validate_busybox(&binary).is_err());
        binary[64] = 1;
        binary[18] = 40;
        assert!(validate_busybox(&binary).is_err());
        assert!(validate_busybox(&elf()[..80]).is_err());
    }

    #[test]
    fn archive_contains_boot_files_and_is_deterministic() {
        let archive = build_archive(&elf()).unwrap();
        assert_eq!(archive, build_archive(&elf()).unwrap());
        assert_eq!(&archive[..6], b"070701");
        for expected in [
            "bin/busybox\0",
            "bin/sh\0",
            "dev/console\0",
            "init\0",
            "TRAILER!!!\0",
        ] {
            assert!(archive
                .windows(expected.len())
                .any(|window| window == expected.as_bytes()));
        }
    }

    #[test]
    fn archive_headers_describe_bootable_entries() {
        use std::collections::BTreeMap;
        let archive = build_archive(&elf()).unwrap();
        let mut entries = BTreeMap::new();
        let mut pos = 0;
        loop {
            assert_eq!(&archive[pos..pos + 6], b"070701");
            let number = |field: usize| {
                let start = pos + 6 + field * 8;
                u32::from_str_radix(std::str::from_utf8(&archive[start..start + 8]).unwrap(), 16)
                    .unwrap() as usize
            };
            let mode = number(1);
            let size = number(6);
            let rdev = (number(9), number(10));
            let name_len = number(11);
            pos += 110;
            let name = std::str::from_utf8(&archive[pos..pos + name_len - 1]).unwrap();
            assert_eq!(archive[pos + name_len - 1], 0);
            pos = (pos + name_len + 3) & !3;
            let contents = &archive[pos..pos + size];
            pos = (pos + size + 3) & !3;
            entries.insert(name.to_string(), (mode, rdev, contents.to_vec()));
            if name == "TRAILER!!!" {
                break;
            }
        }
        assert_eq!(pos, archive.len());
        assert_eq!(entries["bin/sh"].0, 0o120777);
        assert_eq!(entries["bin/sh"].2, b"busybox");
        assert_eq!(entries["dev/console"].1, (5, 1));
        assert_eq!(entries["bin/busybox"].2, elf());
        assert_eq!(entries["init"].2, INIT.as_bytes());
    }

    #[test]
    fn refuses_to_overwrite_output() {
        let dir =
            std::env::temp_dir().join(format!("spacetime-initramfs-test-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let input = dir.join("input");
        let output = dir.join("output");
        fs::write(&input, elf()).unwrap();
        fs::write(&output, b"keep me").unwrap();
        let args = vec![
            "--busybox".to_string(),
            input.to_string_lossy().into_owned(),
            "--out".to_string(),
            output.to_string_lossy().into_owned(),
        ];
        assert!(cmd_initramfs(&args).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"keep me");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parser_requires_unique_flags() {
        let args = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(parse_args(&args(&["--busybox", "b", "--out", "o"])).is_ok());
        assert!(parse_args(&args(&["--busybox", "b"])).is_err());
        assert!(parse_args(&args(&["--busybox", "b", "--busybox", "b", "--out", "o"])).is_err());
        assert!(parse_args(&args(&["--busybox", "--out", "o"])).is_err());
    }
}
