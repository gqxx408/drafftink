# Baseline Memory Test

Measures pure egui + eframe baseline memory on Windows, for comparison with
drafftink-display.exe's 110 MB idle footprint.

## Build

```bash
cargo build --release -p baseline_mem
```

## Measure

1. Launch `target/release/baseline_mem.exe`
2. Wait 3+ seconds for eframe to finish initialising
3. **Method A — Task Manager**:
   Open Task Manager → Details → find `baseline_mem.exe` → note "Private Memory" column
4. **Method B — PowerShell**:
   ```powershell
   Get-Process baseline_mem | Select-Object Name, @{N="PrivateMB";E={[math]::Round($_.PrivateMemorySize64/1MB,1)}}
   ```
   Or run `.\measure.ps1` in this directory.

## Exit

Press **ESC**.

## Result Interpretation

| baseline (MB) | Meaning |
|---|---|
| 40–60 | eframe entry fee is low; drafftink overhead ≈ 50–70 MB (fonts, AnnotationSystem, SmartAlpha, cache) |
| 60–80 | Typical eframe baseline; drafftink overhead ≈ 30–50 MB (healthy) |
| 100–110 | eframe 0.27 itself is this heavy on your GPU driver; zero room for improvement |

### Record

| Program | Private Memory (MB) |
|---|---|
| baseline_mem.exe | **?** |
| display.exe (idle) | 110 |
| **drafftink overhead** | **110 − ?** |
