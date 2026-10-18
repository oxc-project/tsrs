use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use flate2::read::GzDecoder;
use sha2::{Digest as _, Sha512};

use super::Result;

const URL: &str = "https://registry.npmjs.org/@unicode/unicode-15.1.0/-/unicode-15.1.0-1.6.17.tgz";
const INTEGRITY: &str =
    "vhIA4YCwefqQomvbsdkMXVjlTr3OPg99UlRlz04yK6pr2VCrbIA4+4HEbmwx2b92zMwprNGh8Zb6gqf4hE3YXQ==";
const FILES: [&str; 7] = [
    "Simple_Case_Mapping/Lowercase/code-points.js",
    "Simple_Case_Mapping/Uppercase/code-points.js",
    "Special_Casing/Lowercase/code-points.js",
    "Special_Casing/Uppercase/code-points.js",
    "Special_Casing/Lowercase--Final_Sigma/code-points.js",
    "Binary_Property/Cased/ranges.js",
    "Binary_Property/Case_Ignorable/ranges.js",
];

pub fn load(root: &Path) -> Result<BTreeMap<&'static str, String>> {
    let cache = root.join("target/generate-stringutil/unicode-15.1.0-1.6.17.tgz");
    let archive = if cache.is_file() {
        std::fs::read(&cache)?
    } else {
        eprintln!("Downloading @unicode/unicode-15.1.0@1.6.17...");
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .new_agent();
        let bytes = agent.get(URL).call()?.body_mut().read_to_vec()?;
        verify_integrity(&bytes)?;
        std::fs::create_dir_all(cache.parent().unwrap())?;
        std::fs::write(&cache, &bytes)?;
        bytes
    };
    verify_integrity(&archive)?;
    let mut files = BTreeMap::new();
    for entry in tar::Archive::new(GzDecoder::new(archive.as_slice())).entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let Some(name) = FILES.iter().find(|name| path == Path::new(&format!("package/{name}")))
        else {
            continue;
        };
        let mut source = String::new();
        entry.read_to_string(&mut source)?;
        if files.insert(*name, source).is_some() {
            return Err(format!("duplicate npm data file: {name}").into());
        }
    }
    if files.len() != FILES.len() {
        return Err("npm package is missing Unicode data files".into());
    }
    Ok(files)
}

fn verify_integrity(archive: &[u8]) -> Result<()> {
    if BASE64_STANDARD.encode(Sha512::digest(archive)) != INTEGRITY {
        return Err("npm archive does not match the pinned SHA-512 integrity".into());
    }
    Ok(())
}

fn payload<'a>(source: &'a str, prefix: &str, suffix: &str) -> Result<&'a str> {
    source
        .strip_prefix(prefix)
        .and_then(|body| body.strip_suffix(suffix))
        .ok_or_else(|| "unexpected pinned npm data format".into())
}

pub fn scalar(value: u32) -> Result<char> {
    char::from_u32(value).ok_or_else(|| format!("invalid Unicode scalar: {value:#X}").into())
}

// The npm package stores maps as either a JSON literal or base64-encoded gzip JSON.
// Decode those two data wrappers directly; no JavaScript execution is needed.
pub fn mapping(source: &str) -> Result<BTreeMap<u32, String>> {
    let body = payload(source, "module.exports=new Map(", ")")?;
    let json = if body.starts_with("JSON.parse(") {
        let encoded =
            payload(body, "JSON.parse(require('zlib').gunzipSync(Buffer.from('", "','base64')))")?;
        let compressed = BASE64_STANDARD.decode(encoded)?;
        let mut json = String::new();
        GzDecoder::new(compressed.as_slice()).read_to_string(&mut json)?;
        json
    } else {
        body.to_owned()
    };
    let entries: Vec<(u32, serde_json::Value)> = serde_json::from_str(&json)?;
    let mut result = BTreeMap::new();
    for (key, value) in entries {
        scalar(key)?;
        let values: Vec<u32> = if value.is_array() {
            serde_json::from_value(value)?
        } else {
            vec![serde_json::from_value(value)?]
        };
        let text = values.into_iter().map(scalar).collect::<Result<String>>()?;
        if result.insert(key, text).is_some() {
            return Err(format!("duplicate mapping for {key:#X}").into());
        }
    }
    Ok(result)
}

// Port of the package's decode-ranges.js: variable-length 5/10/15/21-bit deltas,
// paired as a gap and a run length. Each previous end is exclusive.
pub fn code_points(source: &str) -> Result<Vec<u32>> {
    let encoded = payload(source, "module.exports=require('../../decode-ranges.js')('", "')")?;
    let mut digits = encoded.bytes().map(|ch| match ch {
        b'A'..=b'Z' => Ok(u32::from(ch - b'A')),
        b'a'..=b'z' => Ok(u32::from(ch - b'a') + 26),
        b'0'..=b'9' => Ok(u32::from(ch - b'0') + 52),
        b'-' => Ok(62),
        b'_' => Ok(63),
        _ => Err("invalid range encoding"),
    });
    let mut deltas = Vec::new();
    while let Some(first) = digits.next() {
        let first = first?;
        let (mut value, extra) = match first {
            0..32 => (first, 0),
            32..48 => (first & 15, 1),
            48..56 => (first & 7, 2),
            _ => (first & 7, 3),
        };
        for _ in 0..extra {
            value = (value << 6) | digits.next().ok_or("truncated range encoding")??;
        }
        deltas.push(value);
    }
    let (pairs, remainder) = deltas.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("unpaired range delta".into());
    }
    let mut next = 0_u32;
    let mut points = Vec::new();
    for &[gap, length] in pairs {
        let start = next.checked_add(gap).ok_or("range overflow")?;
        let end = start.checked_add(length + 1).ok_or("range overflow")?;
        scalar(start)?;
        scalar(end - 1)?;
        for point in start..end {
            scalar(point)?;
            points.push(point);
        }
        next = end + 1;
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_data_and_invalid_scalars() {
        assert_eq!(mapping("module.exports=new Map([[931,[962]]])").unwrap()[&931], "ς");
        assert_eq!(mapping("module.exports=new Map([[65,97]])").unwrap()[&65], "a");
        for input in [
            "module.exports=new Map([[65,97],[65,98]])",
            "module.exports=new Map([[55296,97]])",
            "module.exports=new Map([[65,[1114112]]])",
            "module.exports=new Map([[65,1.5]])",
            "module.exports=new Map([[65,-1]])",
            "module.exports=new Map(truncated)",
        ] {
            assert!(mapping(input).is_err(), "{input}");
        }
        assert!(verify_integrity(b"modified archive").is_err());
    }

    #[test]
    fn decodes_range_gaps_and_rejects_truncation() {
        let wrap =
            |data: &str| format!("module.exports=require('../../decode-ranges.js')('{data}')");
        assert_eq!(code_points(&wrap("ABAC")).unwrap(), [0, 1, 3, 4, 5]);
        assert_eq!(code_points(&wrap("hBA")).unwrap(), [65]);
        assert_eq!(code_points(&wrap("wQAA")).unwrap(), [1024]);
        assert_eq!(code_points(&wrap("4QAAA")).unwrap(), [65536]);
        for data in ["?A", "h", "wQ", "A"] {
            assert!(code_points(&wrap(data)).is_err(), "{data}");
        }
    }
}
