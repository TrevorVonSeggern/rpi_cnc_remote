use std::str::FromStr;

use crate::state::RemoteEvent;

#[derive(Debug, PartialEq, Eq)]
pub enum ParseRemoteEventError {
    ParseError,
    BadStartingId,
}
impl FromStr for RemoteEvent {
    type Err = ParseRemoteEventError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.len() <= 4 {
            return Err(ParseRemoteEventError::BadStartingId);
        }
        let data_part = &input[2..input.len()-1];
        match &input[..2] {
            "W:" => data_part.parse().map(|p| RemoteEvent::DialXYZEvent(p)).map_err(|_| ParseRemoteEventError::ParseError),
            "L:" => {
                if let Some((skip, dir)) = data_part.split_once(" ") {
                    if let Ok(skip) = skip.parse() {
                        Ok(RemoteEvent::SDList((dir.to_string(), skip)))
                    }
                    else {
                        Err(ParseRemoteEventError::ParseError)
                    }
                }
                else {
                    Err(ParseRemoteEventError::ParseError)
                }
            },
            "F:" => Ok(RemoteEvent::SDLoadFile(data_part.to_string())),
            "G:" => Ok(RemoteEvent::RunGCode(data_part.to_string())),
            _ => Err(ParseRemoteEventError::BadStartingId),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xyz_zero_int() {
        let state = "W:X0 Y0 Z0\n".parse().expect("parse state");
        if let RemoteEvent::DialXYZEvent(p) = state {
            assert_eq!(p.x, 0);
            assert_eq!(p.y, 0);
            assert_eq!(p.z, 0);
        }
        else {
            assert!(false, "bad event parsed.")
        }
    }

    #[test]
    fn parse_gcode() {
        let state = "G:G91\n".parse().expect("parse success");
        if let RemoteEvent::RunGCode(code) = state {
            assert_eq!(code, "G91");
        }
        else {
            assert!(false, "bad event parsed.")
        }
    }

    #[test]
    fn parse_sd_list_root() {
        let state = "L:\n".parse().expect("parse success");
        if let RemoteEvent::SDList(root) = state {
            assert_eq!(root, "");
        }
        else {
            assert!(false, "bad event parsed.")
        }
    }

    #[test]
    fn parse_sd_list_non_root() {
        let state = "L:/D/directory\n".parse().expect("parse success");
        if let RemoteEvent::SDList(root) = state {
            assert_eq!(root, "/D/directory");
        }
        else {
            assert!(false, "bad event parsed.")
        }
    }

    #[test]
    fn parse_run_file() {
        let state = "F:/D/file.nc\n".parse().expect("parse success");
        if let RemoteEvent::SDList(root) = state {
            assert_eq!(root, "/D/file.nc");
        }
        else {
            assert!(false, "bad event parsed.")
        }
    }
}
