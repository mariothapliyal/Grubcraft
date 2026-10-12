// metal-floor: the smallest possible Metal window, to measure what macOS and
// the GPU driver cost before Grubcraft adds anything. It opens a window the
// same size as the game's, clears it every frame, and prints process memory
// (the figure Activity Monitor shows) at the same milestones as the game.
//
//   metal-floor [--frames N]
//
// Built into Grubcraft.app/Contents/MacOS by scripts/bundle-macos.sh.

import AppKit
import Metal
import QuartzCore

func footprintBytes() -> UInt64? {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(
        MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
        }
    }
    return kr == KERN_SUCCESS ? info.phys_footprint : nil
}

var previous: UInt64?

func mark(_ stage: String) {
    let name = stage.padding(toLength: 26, withPad: " ", startingAt: 0)
    guard let now = footprintBytes() else {
        fputs("  \(name)        ?\n", stderr)
        return
    }
    let mb = Double(now) / 1_048_576
    var line = "  \(name) " + String(format: "%8.1f", mb)
    if let p = previous {
        line += String(format: "  (%+.1f)", (Double(now) - Double(p)) / 1_048_576)
    }
    fputs(line + "\n", stderr)
    previous = now
}

final class Floor: NSObject, NSApplicationDelegate {
    let frames: Int
    var window: NSWindow!
    var layer: CAMetalLayer!
    var queue: MTLCommandQueue!
    var frame = 0

    init(frames: Int) {
        self.frames = frames
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        window = NSWindow(
            contentRect: NSRect(x: 120, y: 120, width: 1280, height: 760),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered, defer: false)
        window.title = "metal-floor"
        let scale = window.backingScaleFactor
        guard let device = MTLCreateSystemDefaultDevice() else {
            fputs("metal-floor: no Metal device\n", stderr)
            exit(1)
        }
        queue = device.makeCommandQueue()
        layer = CAMetalLayer()
        layer.device = device
        layer.pixelFormat = .bgra8Unorm_srgb
        layer.framebufferOnly = true
        layer.contentsScale = scale
        layer.drawableSize = CGSize(width: 1280 * scale, height: 760 * scale)
        layer.maximumDrawableCount = 3
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 1280, height: 760))
        view.wantsLayer = true
        view.layer = layer
        window.contentView = view
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        fputs(
            "metal-floor: \(device.name), \(Int(1280 * scale))x\(Int(760 * scale)) pixels, 3 drawables\n",
            stderr)
        mark("window + metal device")
        Timer.scheduledTimer(
            timeInterval: 1.0 / 240.0, target: self, selector: #selector(tick),
            userInfo: nil, repeats: true)
    }

    @objc func tick() {
        autoreleasepool {
            guard let drawable = layer.nextDrawable(),
                let commands = queue.makeCommandBuffer()
            else { return }
            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = drawable.texture
            pass.colorAttachments[0].loadAction = .clear
            pass.colorAttachments[0].storeAction = .store
            pass.colorAttachments[0].clearColor = MTLClearColor(
                red: 0.34, green: 0.39, blue: 0.45, alpha: 1)
            commands.makeRenderCommandEncoder(descriptor: pass)?.endEncoding()
            commands.present(drawable)
            commands.commit()
        }
        frame += 1
        switch frame {
        case 1: mark("frame 1")
        case 2: mark("frame 2")
        case 10: mark("frame 10")
        case 120: mark("frame 120")
        default: break
        }
        if frame >= frames {
            mark("exit")
            exit(0)
        }
    }
}

var frames = 300
let args = CommandLine.arguments
if args.count == 3, args[1] == "--frames", let n = Int(args[2]), n > 0 {
    frames = n
} else if args.count != 1 {
    fputs("usage: metal-floor [--frames N]\n", stderr)
    exit(2)
}
mark("process start")
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = Floor(frames: frames)
app.delegate = delegate
app.run()
