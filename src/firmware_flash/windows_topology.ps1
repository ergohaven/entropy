$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
function Property($id, $key) {
    (Get-PnpDeviceProperty -InstanceId $id -KeyName $key -ErrorAction Stop).Data
}
function UsbParent($id) {
    for ($depth = 0; $depth -lt 16 -and $id; $depth++) {
        if ($id -match '^USB\\VID_[0-9A-F]{4}&PID_[0-9A-F]{4}\\') { return $id }
        $id = Property $id 'DEVPKEY_Device_Parent'
    }
    return $null
}
$volumes = @{}
foreach ($disk in @(Get-CimInstance Win32_DiskDrive)) {
    if ($disk.PNPDeviceID -notmatch '^USBSTOR\\') { continue }
    $usb = UsbParent $disk.PNPDeviceID
    if (!$usb) { continue }
    $roots = @()
    foreach ($partition in @(Get-CimAssociatedInstance -InputObject $disk -Association Win32_DiskDriveToDiskPartition)) {
        foreach ($drive in @(Get-CimAssociatedInstance -InputObject $partition -Association Win32_LogicalDiskToPartition)) {
            $roots += ($drive.DeviceID + '\')
        }
    }
    $volumes[$usb] = @($roots | Select-Object -Unique)
}
$result = @()
foreach ($device in @(Get-PnpDevice -PresentOnly)) {
    if ($device.InstanceId -notmatch '^USB\\VID_([0-9A-F]{4})&PID_([0-9A-F]{4})\\([^\\]+)$') { continue }
    $vendor = [Convert]::ToInt32($Matches[1], 16)
    $product = [Convert]::ToInt32($Matches[2], 16)
    if (!(($vendor -eq 0xE126 -and $product -ge 0x52 -and $product -le 0x54) -or ($vendor -eq 0x2E8A -and $product -eq 3))) { continue }
    $locations = @(Property $device.InstanceId 'DEVPKEY_Device_LocationPaths')
    if ($locations.Count -lt 1 -or !$locations[0]) { throw 'USB location unavailable' }
    $roots = @()
    if ($volumes.ContainsKey($device.InstanceId)) { $roots = @($volumes[$device.InstanceId]) }
    $result += @{port=('windows:' + $locations[0]); vendor=$vendor; product=$product; mounts=@($roots)}
}
ConvertTo-Json -InputObject @($result) -Depth 4 -Compress
