param([Parameter(Mandatory=$true)][string]$BinaryPath)
$binaryBytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $BinaryPath).Path)
function UInt16At([int]$Offset) { [BitConverter]::ToUInt16($binaryBytes, $Offset) }
function UInt32At([int]$Offset) { [BitConverter]::ToUInt32($binaryBytes, $Offset) }
$peOffset = UInt32At 0x3c
if ((UInt32At $peOffset) -ne 0x4550) { throw 'Not a PE executable' }
$sectionCount = UInt16At ($peOffset + 6)
$optionalOffset = $peOffset + 24
$optionalSize = UInt16At ($peOffset + 20)
$directoryOffset = if ((UInt16At $optionalOffset) -eq 0x20b) { $optionalOffset + 112 } else { $optionalOffset + 96 }
$sections = @()
for ($i = 0; $i -lt $sectionCount; $i++) {
    $offset = $optionalOffset + $optionalSize + $i * 40
    $sections += @{Address=(UInt32At ($offset+12)); Size=[Math]::Max((UInt32At ($offset+8)), (UInt32At ($offset+16))); Pointer=(UInt32At ($offset+20))}
}
function ResolveRva([uint32]$Rva) {
    foreach ($section in $sections) { if ($Rva -ge $section.Address -and $Rva -lt $section.Address + $section.Size) { return [int]($section.Pointer + $Rva - $section.Address) } }
    throw "Unmapped RVA $Rva"
}
$importRva = UInt32At ($directoryOffset + 8)
$imports = @()
if ($importRva -ne 0) {
    $descriptor = ResolveRva $importRva
    while ((UInt32At ($descriptor + 12)) -ne 0) {
        $nameOffset = ResolveRva (UInt32At ($descriptor + 12))
        $end = $nameOffset
        while ($binaryBytes[$end] -ne 0) { $end++ }
        $imports += [Text.Encoding]::ASCII.GetString($binaryBytes, $nameOffset, $end - $nameOffset)
        $descriptor += 20
    }
}
[pscustomobject]@{File=$BinaryPath; Bytes=$binaryBytes.Length; Imports=$imports} | ConvertTo-Json
