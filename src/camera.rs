use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const RECORDINGS_DIR: &str = "Videos/Recordings";

static RECORDING_PROCESS: Mutex<Option<(Child, ChildStdin)>> = Mutex::new(None);



#[cfg(target_os = "linux")]
pub fn list_cameras() -> Result<Vec<String>, io::Error> {
    let mut cameras = Vec::new();

    for i in 0..4 {
        let device = format!("/dev/video{}", i);

        if Path::new(&device).exists() {
            cameras.push(device);
        }
    }

    Ok(cameras)
}



#[cfg(target_os = "macos")]
pub fn list_cameras() -> Result<Vec<String>, io::Error> {
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .output()
        .map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("Failed to run FFmpeg: {}", e))
        })?;

    let output = String::from_utf8_lossy(&output.stderr);

    let mut cameras = Vec::new();
    let mut video_devices = false;

    for line in output.lines() {
        if line.contains("AVFoundation video devices") {
            video_devices = true;
            continue;
        }

        if line.contains("AVFoundation audio devices") {
            break;
        }

        if video_devices {
            if let Some(end) = line.find(']') {
                let name = line[end + 1..].trim();

                if !name.is_empty() {
                    cameras.push(name.to_string());
                }
            }
        }
    }

    Ok(cameras)
}



pub fn start_camera(camera: u8) -> Result<(), io::Error> {
    fs::create_dir_all(RECORDINGS_DIR)?;

    // if the camera specified is in use then kill it and save that recording then start a new recording
    if let Some((mut process, mut stdin)) = RECORDING_PROCESS.lock().unwrap().take() {
        // ffmpeg stop now
        stdin.write_all(b"q\n")?;
        stdin.flush()?;

        // wait til process complete then start a new recording
        process.wait()?;
    }


    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Invalid system time"))?
        .as_secs();

    let filename = format!("recording_{}.mp4", timestamp);
    let output = Path::new(RECORDINGS_DIR).join(filename);

    let (process, stdin) = start_ffmpeg(camera, &output)?;

    *RECORDING_PROCESS.lock().unwrap() = Some((process, stdin));

    Ok(())
}



#[cfg(target_os = "linux")]
fn start_ffmpeg(camera: u8, output: &Path) -> Result<(Child, ChildStdin), io::Error> {
    let device = format!("/dev/video{}", camera);

    let mut process = Command::new("ffmpeg")
        .args(generate_ffmpeg_command(&device, output))
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to start FFmpeg: {}", e),
            )
        })?;

    let stdin = process
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to access FFmpeg stdin"))?;

    Ok((process, stdin))
}



#[cfg(target_os = "macos")]
fn start_ffmpeg(camera: u8, output: &Path) -> Result<(Child, ChildStdin), io::Error> {
    let device = format!("{}:none", camera);

    let mut process = Command::new("ffmpeg")
        .args(generate_ffmpeg_command(&device, output))
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to start FFmpeg: {}", e),
            )
        })?;

    let stdin = process
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to access FFmpeg stdin"))?;

    Ok((process, stdin))
}



pub fn stop_camera() -> Result<(), io::Error> {
    let mut recording = RECORDING_PROCESS.lock().unwrap();

    if let Some((mut process, mut stdin)) = recording.take() {
        // Tell FFmpeg to stop cleanly.
        stdin.write_all(b"q\n")?;
        stdin.flush()?;

        // Wait for FFmpeg to finish writing the MP4.
        process.wait()?;
    }

    Ok(())
}



pub fn delete_video(video_name: &str) -> Result<(), io::Error> {
    let path = Path::new(RECORDINGS_DIR).join(video_name);

    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Video not found"));
    }

    fs::remove_file(path)?;

    Ok(())
}



pub fn list_recordings() -> Vec<String> {
    let entries = match fs::read_dir(RECORDINGS_DIR) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".mp4"))
        .collect()
}



#[cfg(target_os = "linux")]
fn generate_ffmpeg_command(device: &str, output: &Path) -> Vec<String> {
    vec![
        "-f".into(),
        "v4l2".into(),
        "-video_size".into(),
        "320x240".into(),
        "-framerate".into(),
        "30".into(),
        "-i".into(),
        device.into(),
        "-vf".into(),
        "fps=10,format=gray".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "32".into(),
        "-an".into(),
        "-y".into(),
        output.to_string_lossy().into_owned(),
    ]
}

#[cfg(target_os = "macos")]
fn generate_ffmpeg_command(device: &str, output: &Path) -> Vec<String> {
    vec![
        "-f".into(),
        "avfoundation".into(),
        "-video_size".into(),
        "320x240".into(),
        "-framerate".into(),
        "30".into(),
        "-i".into(),
        device.into(),
        "-vf".into(),
        "fps=10,format=gray".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "32".into(),
        "-an".into(),
        "-y".into(),
        output.to_string_lossy().into_owned(),
    ]
}