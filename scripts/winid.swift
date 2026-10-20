import CoreGraphics
import Foundation
// Prints the id of the game's real window: owned by the process and large (the process
// also owns a few tiny helper windows, and the title is absent when it is on another Space).
// An on-screen window wins over a bigger one on another Space: Adam's shipped build is often
// open fullscreen while a dev capture runs, and screencapture cannot grab an offscreen window.
let name = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "wingwright"
let list = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
var best: (Int, Double, Bool)? = nil
for w in list {
    let owner = (w[kCGWindowOwnerName as String] as? String) ?? ""
    let layer = (w[kCGWindowLayer as String] as? Int) ?? 0
    guard layer == 0, owner.lowercased() == name.lowercased() else { continue }
    let b = w[kCGWindowBounds as String] as? [String: Any]
    let wd = (b?["Width"] as? Double) ?? 0
    let ht = (b?["Height"] as? Double) ?? 0
    guard wd >= 800, ht >= 500 else { continue }
    let on = (w[kCGWindowIsOnscreen as String] as? Bool) ?? false
    let better = best == nil || (on && !best!.2) || (on == best!.2 && wd * ht > best!.1)
    if better { best = (w[kCGWindowNumber as String] as! Int, wd * ht, on) }
}
if let b = best { print(b.0) }
