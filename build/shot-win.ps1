param(
  [int]$Left = 1146,
  [int]$Top = 243,
  [int]$Width = 1456,
  [int]$Height = 939,
  [string]$Out = "D:\Grisia Studio\Grisia Studio\build\grok-window.png"
)
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$bmp = New-Object System.Drawing.Bitmap $Width, $Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($Left, $Top, 0, 0, $bmp.Size)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
"Wrote $Out"
