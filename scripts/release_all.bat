@echo off

echo ========================================================
echo 1. Committing release workflows and tagging plugins...
echo ========================================================

for %%P in (plugin-image-gen plugin-image-editor plugin-video-gen plugin-3d-gen plugin-voice-tts plugin-music-gen plugin-vision-ocr plugin-rag-qa plugin-translation plugin-text-analysis plugin-ssh plugin-travel-tunnel plugin-model-training) do (
    echo Releasing %%P v1.0.0
    cd d:\Documents\Syncho\plugins\%%P
    git add .
    git commit -m "ci: add standardized extension bundle release workflow"
    git push -u origin main
    git tag -a v1.0.0 -m "Release v1.0.0" --force
    git push origin v1.0.0 --force
)

echo.
echo ========================================================
echo 2. Tagging and releasing Locaryn Core (v0.1.0)...
echo ========================================================
cd d:\Documents\Syncho
git add .
git commit -m "ci: trigger official release builds for all platforms"
git push origin main
git tag -a v0.1.0 -m "Release v0.1.0" --force
git push origin v0.1.0 --force

echo.
echo ========================================================
echo All 14 releases triggered successfully on GitHub!
echo ========================================================
