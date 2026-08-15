import os
from PIL import Image

desktop_icon_path = r"d:\Documents\Syncho\apps\desktop\src-tauri\icons\icon.png"
if not os.path.exists(desktop_icon_path):
    print(f"Error: {desktop_icon_path} not found")
    exit(1)

src_img = Image.open(desktop_icon_path).convert("RGBA")

# Copy to mobile icons folder
mobile_icons_dir = r"d:\Documents\Syncho\apps\mobile\src-tauri\icons"
os.makedirs(mobile_icons_dir, exist_ok=True)
for icon_name in ["icon.png", "32x32.png", "128x128.png", "128x128@2x.png", "icon.ico", "icon.icns"]:
    desktop_file = os.path.join(r"d:\Documents\Syncho\apps\desktop\src-tauri\icons", icon_name)
    if os.path.exists(desktop_file):
        with open(desktop_file, "rb") as f_in, open(os.path.join(mobile_icons_dir, icon_name), "wb") as f_out:
            f_out.write(f_in.read())

# Android Mipmap dimensions
# Standard launcher: mdpi 48, hdpi 72, xhdpi 96, xxhdpi 144, xxxhdpi 192
# Foreground: mdpi 108, hdpi 162, xhdpi 216, xxhdpi 324, xxxhdpi 432
densities = {
    "mipmap-mdpi": (48, 108),
    "mipmap-hdpi": (72, 162),
    "mipmap-xhdpi": (96, 216),
    "mipmap-xxhdpi": (144, 324),
    "mipmap-xxxhdpi": (192, 432),
}

res_dir = r"d:\Documents\Syncho\apps\mobile\src-tauri\gen\android\app\src\main\res"

for folder, (size, fg_size) in densities.items():
    target_dir = os.path.join(res_dir, folder)
    if not os.path.exists(target_dir):
        continue

    # 1. ic_launcher.png
    launcher = src_img.resize((size, size), Image.Resampling.LANCZOS)
    launcher.save(os.path.join(target_dir, "ic_launcher.png"), "PNG")
    launcher.save(os.path.join(target_dir, "ic_launcher_round.png"), "PNG")

    # 2. ic_launcher_foreground.png (scaled centered on fg canvas)
    fg_canvas = Image.new("RGBA", (fg_size, fg_size), (0, 0, 0, 0))
    # Android adaptive icon inner safe zone is ~66% of the full canvas
    inner_size = int(fg_size * 0.72)
    inner_img = src_img.resize((inner_size, inner_size), Image.Resampling.LANCZOS)
    offset = (fg_size - inner_size) // 2
    fg_canvas.paste(inner_img, (offset, offset), inner_img)
    fg_canvas.save(os.path.join(target_dir, "ic_launcher_foreground.png"), "PNG")

    print(f"Updated {folder}: launcher {size}x{size}, foreground {fg_size}x{fg_size}")

# Also copy an official 512x512 logo at project root and assets
os.makedirs(r"d:\Documents\Syncho\assets", exist_ok=True)
src_img.resize((512, 512), Image.Resampling.LANCZOS).save(r"d:\Documents\Syncho\assets\locaryn-logo.png", "PNG")
src_img.resize((512, 512), Image.Resampling.LANCZOS).save(r"d:\Documents\Syncho\assets\github-org-avatar.png", "PNG")
print("Saved assets/locaryn-logo.png and assets/github-org-avatar.png for GitHub organization profile!")
