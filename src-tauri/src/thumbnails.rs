use image::imageops::FilterType;
use image::{ImageDecoder, ImageReader, Limits};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024; // 50 MB max per image file
const MAX_DIMENSION: u32 = 10000; // Reject decompression bomb images > 10000x10000

pub struct ProcessedImageInfo {
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub content_hash: String,
    pub thumbnail_path: String,
}

pub fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "bmp"
        )
    } else {
        false
    }
}

/// Computes BLAKE3 hash of file content, validates image dimensions against decompression bombs,
/// and creates a memory-efficient cached thumbnail.
pub fn process_and_thumbnail(
    source_path: &Path,
    thumbs_dir: &Path,
) -> Result<ProcessedImageInfo, String> {
    if !source_path.exists() || !source_path.is_file() {
        return Err("File does not exist".into());
    }

    let metadata = std::fs::metadata(source_path).map_err(|e| e.to_string())?;
    let file_size = metadata.len();
    if file_size == 0 || file_size > MAX_FILE_BYTES {
        return Err("Image file exceeds 50 MB safety limit or is empty".into());
    }

    // Compute fast BLAKE3 hash
    let mut file = File::open(source_path).map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let content_hash = hasher.finalize().to_hex().to_string();

    std::fs::create_dir_all(thumbs_dir).map_err(|e| e.to_string())?;
    let thumb_file_path: PathBuf = thumbs_dir.join(format!("{}.jpg", &content_hash[..24]));

    // Configure strict decoder memory limits to prevent decompression bombs & RAM spikes
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(256 * 1024 * 1024); // 256 MB max decode buffer

    let mut reader = ImageReader::open(source_path)
        .map_err(|e| format!("Failed to open image: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to guess image format: {}", e))?;
    reader.limits(limits);

    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Invalid image header: {}", e))?;
    let (width, height) = decoder.dimensions();

    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err("Image dimensions out of safe bounds".into());
    }

    if !thumb_file_path.exists() {
        let img = image::DynamicImage::from_decoder(decoder)
            .map_err(|e| format!("Unsupported or corrupted image file: {}", e))?;

        // 640x360 thumbnail uses ~60% less RAM in WebView2 GPU texture cache while staying crisp in slices
        let thumb = img.resize(640, 360, FilterType::Triangle);
        thumb
            .save(&thumb_file_path)
            .map_err(|e| format!("Failed to save thumbnail: {}", e))?;
    }

    Ok(ProcessedImageInfo {
        width,
        height,
        file_size,
        content_hash,
        thumbnail_path: thumb_file_path.to_string_lossy().to_string(),
    })
}
