# Relais Windows : rend l'imprimante USB accessible au dev container.
# Chaque connexion TCP reçue devient un travail d'impression brut (RAW) sur la file Windows,
# sans passer par le rendu du pilote. Depuis le conteneur :
#   cargo run -- --tcp host.docker.internal:9101 print examples/matin.json
# Usage : powershell -ExecutionPolicy Bypass -File scripts\windows-relay.ps1 [-Printer "nom"] [-Port 9101]
param(
    [string]$Printer = "EPSON TM-T88V Receipt",
    [int]$Port = 9101
)
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class RawPrinter {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public class DOCINFO { public string pDocName; public string pOutputFile; public string pDataType; }
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool OpenPrinter(string name, out IntPtr h, IntPtr d);
    [DllImport("winspool.drv", SetLastError = true)] static extern bool ClosePrinter(IntPtr h);
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern int StartDocPrinter(IntPtr h, int level, DOCINFO di);
    [DllImport("winspool.drv", SetLastError = true)] static extern bool EndDocPrinter(IntPtr h);
    [DllImport("winspool.drv", SetLastError = true)] static extern bool StartPagePrinter(IntPtr h);
    [DllImport("winspool.drv", SetLastError = true)] static extern bool EndPagePrinter(IntPtr h);
    [DllImport("winspool.drv", SetLastError = true)]
    static extern bool WritePrinter(IntPtr h, byte[] buf, int len, out int written);

    public static void Send(string printer, byte[] data) {
        IntPtr h;
        if (!OpenPrinter(printer, out h, IntPtr.Zero)) throw new Win32Exception();
        try {
            if (StartDocPrinter(h, 1, new DOCINFO { pDocName = "printr", pDataType = "RAW" }) == 0)
                throw new Win32Exception();
            try {
                StartPagePrinter(h);
                int written;
                if (!WritePrinter(h, data, data.Length, out written)) throw new Win32Exception();
                EndPagePrinter(h);
            } finally { EndDocPrinter(h); }
        } finally { ClosePrinter(h); }
    }
}
'@

Get-Printer -Name $Printer | Out-Null  # échoue tout de suite si la file n'existe pas

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
$listener.Start()
Write-Host "Relais 127.0.0.1:$Port -> '$Printer' (Ctrl+C pour arrêter)"
try {
    while ($true) {
        # Pending() plutôt qu'un Accept bloquant, pour que Ctrl+C reste immédiat.
        if (-not $listener.Pending()) { Start-Sleep -Milliseconds 100; continue }
        $client = $listener.AcceptTcpClient()
        try {
            $buffer = [System.IO.MemoryStream]::new()
            $client.GetStream().CopyTo($buffer)  # le ticket se termine à la fermeture de la connexion
            if ($buffer.Length -gt 0) {
                [RawPrinter]::Send($Printer, $buffer.ToArray())
                Write-Host ("{0:HH:mm:ss}  {1} octets imprimés" -f (Get-Date), $buffer.Length)
            }
        } catch {
            Write-Warning $_.Exception.Message
        } finally {
            $client.Close()
        }
    }
} finally {
    $listener.Stop()
}
