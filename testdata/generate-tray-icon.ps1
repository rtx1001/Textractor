Add-Type -AssemblyName System.Drawing
$bitmap = [System.Drawing.Bitmap]::new(128, 128)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.Clear([System.Drawing.Color]::Transparent)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.ScaleTransform(128 / 24, 128 / 24)
$pen = [System.Drawing.Pen]::new([System.Drawing.ColorTranslator]::FromHtml('#F2BE8D'), 1.8)
$pen.StartCap = $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
foreach ($corner in 0..3) {
    $state = $graphics.Save()
    $graphics.TranslateTransform(12, 12)
    $graphics.RotateTransform(90 * $corner)
    $graphics.TranslateTransform(-12, -12)
    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $path.AddLine(2, 6, 2, 4)
    $path.AddArc(2, 2, 4, 4, 180, 90)
    $path.AddLine(4, 2, 6, 2)
    $graphics.DrawPath($pen, $path)
    $path.Dispose()
    $graphics.Restore($state)
}
$graphics.DrawLine($pen, 6, 7, 16, 7)
$graphics.DrawLine($pen, 6, 12, 18, 12)
$graphics.DrawLine($pen, 6, 17, 14, 17)
$small = [System.Drawing.Bitmap]::new(32, 32)
$outputGraphics = [System.Drawing.Graphics]::FromImage($small)
$outputGraphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$outputGraphics.DrawImage($bitmap, 0, 0, 32, 32)
$small.Save((Join-Path $PSScriptRoot '../src-tauri/icons/tray-icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$outputGraphics.Dispose(); $small.Dispose(); $pen.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
