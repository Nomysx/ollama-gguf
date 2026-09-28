# run the tests, build the release exe and copy it to dist
Set-Location $PSScriptRoot

cargo test --quiet
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\ollama-gguf.exe dist\ -Force
Write-Host "done: dist\ollama-gguf.exe"
