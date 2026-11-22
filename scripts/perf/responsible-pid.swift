// usage: swift responsible-pid.swift <pid>...   prints "<pid> <responsible pid>" per pid (what Activity Monitor groups by).
// Exits 3 when the API is missing. A responsible pid of -1 means the lookup failed for that pid.
import Darwin
typealias F = @convention(c) (pid_t) -> pid_t
guard let h = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid") else { print("unavailable"); exit(3) }
let f = unsafeBitCast(h, to: F.self)
for a in CommandLine.arguments.dropFirst() { if let p = Int32(a) { print("\(p) \(f(p))") } }
