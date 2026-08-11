use chrono::Local;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;


const RECORDINGS_DIR: &str = "Videos/Recordings";


static RECORDING_PROCESSES: Mutex<Vec<(Child, ChildStdin)>> = Mutex::new(Vec::new());




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
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to run FFmpeg: {}", e),
            )
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

    Ok(cameras.iter().filter(|c| c.contains("USB")).cloned().collect::<Vec<_>>())
}




pub fn start_recording(cameras: &[u8]) -> Result<(), io::Error> {
    fs::create_dir_all(RECORDINGS_DIR)?;

    stop_recording()?;

    for camera in cameras {
        let timestamp = Local::now().format("%d-%m-%Y-%H-%M-%S");


        let filename = format!(
            "{}-camera-{}.mp4",
            timestamp,
            camera + 1
        );


        let output = Path::new(RECORDINGS_DIR).join(filename);


        let recording = start_ffmpeg(*camera, &output)?;


        RECORDING_PROCESSES
            .lock()
            .unwrap()
            .push(recording);
    }


    Ok(())
}




#[cfg(target_os = "linux")]
fn start_ffmpeg(
    camera: u8,
    output: &Path,
) -> Result<(Child, ChildStdin), io::Error> {
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
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "Failed to access FFmpeg stdin",
            )
        })?;


    Ok((process, stdin))
}




#[cfg(target_os = "macos")]
fn start_ffmpeg(
    camera: u8,
    output: &Path,
) -> Result<(Child, ChildStdin), io::Error> {
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
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "Failed to access FFmpeg stdin",
            )
        })?;


    Ok((process, stdin))
}




pub fn stop_recording() -> Result<(), io::Error> {
    let mut recordings = RECORDING_PROCESSES
        .lock()
        .unwrap()
        .drain(..)
        .collect::<Vec<_>>();


    for (_, stdin) in &mut recordings {
        stdin.write_all(b"q\n")?;
        stdin.flush()?;
    }


    for (mut process, _) in recordings {
        process.wait()?;
    }


    Ok(())
}



pub fn delete_video(video_name: &str) -> Result<(), io::Error> {
    let path = Path::new(RECORDINGS_DIR).join(video_name);


    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Video not found",
        ));
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
        .filter_map(|entry| {
            entry
                .ok()?
                .file_name()
                .into_string()
                .ok()
        })
        .filter(|name| name.ends_with(".mp4"))
        .collect()
}




#[cfg(target_os = "linux")]
fn generate_ffmpeg_command(
    device: &str,
    output: &Path,
) -> Vec<String> {
    vec![
        "-f".into(),
        "v4l2".into(),
        "-video_size".into(),
        "640x480".into(),
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
fn generate_ffmpeg_command(
    device: &str,
    output: &Path,
) -> Vec<String> {
    vec![
        "-f".into(),
        "avfoundation".into(),
        "-video_size".into(),
        "640x480".into(),
        "-framerate".into(),
        "30".into(),
        "-i".into(),
        device.into(),
        "-vf".into(),
        "fps=30".into(),
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