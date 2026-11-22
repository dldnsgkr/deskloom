// Usage: swift scripts/dev/list-windows.swift <pid>
// Lists on-screen windows of a process: layer and bounds (no Screen Recording permission needed).
import CoreGraphics
import Foundation

guard CommandLine.arguments.count > 1, let pid = Int32(CommandLine.arguments[1]) else {
  print("usage: list-windows.swift <pid>")
  exit(1)
}
let list = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as! [[String: Any]]
for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid {
  let b = w[kCGWindowBounds as String] as! [String: Any]
  print("layer=\(w[kCGWindowLayer as String]!) x=\(b["X"]!) y=\(b["Y"]!) w=\(b["Width"]!) h=\(b["Height"]!)")
}
