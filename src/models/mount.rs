use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MountPoint {
    pub name: String,
    pub path: String,
}

impl MountPoint {
    /// Parse mount points from "name1:/path1,name2:/path2" format
    pub fn parse_all(raw: &str) -> Vec<MountPoint> {
        if raw.is_empty() {
            return Vec::new();
        }
        raw.split(',')
            .filter_map(|entry| {
                let entry = entry.trim();
                let (name, path) = entry.split_once(':')?;
                let name = name.trim().to_string();
                let path = path.trim().to_string();
                if name.is_empty() || path.is_empty() {
                    return None;
                }
                Some(MountPoint { name, path })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_empty_string_returns_empty_vec() {
        let result = MountPoint::parse_all("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_all_single_mount() {
        let result = MountPoint::parse_all("docs:/data/docs");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "docs");
        assert_eq!(result[0].path, "/data/docs");
    }

    #[test]
    fn parse_all_multiple_mounts() {
        let result = MountPoint::parse_all("docs:/data/docs,photos:/data/photos");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "docs");
        assert_eq!(result[0].path, "/data/docs");
        assert_eq!(result[1].name, "photos");
        assert_eq!(result[1].path, "/data/photos");
    }

    #[test]
    fn parse_all_whitespace_handling() {
        let result = MountPoint::parse_all("  docs : /data/docs , photos : /data/photos  ");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "docs");
        assert_eq!(result[0].path, "/data/docs");
        assert_eq!(result[1].name, "photos");
        assert_eq!(result[1].path, "/data/photos");
    }

    #[test]
    fn parse_all_missing_colon_is_skipped() {
        let result = MountPoint::parse_all("invalid_entry");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_all_empty_name_is_skipped() {
        let result = MountPoint::parse_all(":/data/docs");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_all_empty_path_is_skipped() {
        let result = MountPoint::parse_all("docs:");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_all_mixed_valid_and_invalid() {
        let result =
            MountPoint::parse_all("docs:/data/docs,invalid_entry,:/empty_name,photos:/data/photos");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "docs");
        assert_eq!(result[1].name, "photos");
    }
}
