//! 构建时生成应用图标 icon.ico 并嵌入 Windows 可执行文件（任务栏/桌面/资源管理器显示）

const R: u8 = 217;
const G: u8 = 17;
const B: u8 = 83;

fn make_rgba_circle(size: u32) -> Vec<u8> {
    let cx = (size as f32) * 0.5;
    let cy = (size as f32) * 0.5;
    let r = (size as f32) * 0.44;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32) + 0.5 - cx;
            let dy = (y as f32) + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r {
                rgba.extend_from_slice(&[R, G, B, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

fn main() {
    #[cfg(windows)]
    {
        let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let icon_path = manifest_dir.join("icon.ico");

        let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
        for &size in &[16u32, 32u32, 48u32] {
            let rgba = make_rgba_circle(size);
            let image = ico::IconImage::from_rgba_data(size, size, rgba);
            let entry = ico::IconDirEntry::encode(&image).expect("encode icon entry");
            icon_dir.add_entry(entry);
        }

        let mut file = std::fs::File::create(&icon_path).expect("create icon.ico");
        icon_dir.write(&mut file).expect("write icon.ico");

        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("winres: {} (若未装 Windows SDK/rc.exe，可忽略，图标将不嵌入 exe)", e);
        }
    }
}
