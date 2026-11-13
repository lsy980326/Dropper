import Foundation
import CoreGraphics

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else { exit(2) }
let windows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []
guard windows.contains(where: { ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid && ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0 }) else {
    fputs("Installed application did not create an on-screen window.\n", stderr)
    exit(1)
}
print("Verified installed native application window for PID \(pid).")
