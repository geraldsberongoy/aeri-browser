// Renders the static Finder background of the Aeri disk image.
// Usage: swift background.swift <glyph.png> <out.png> <scale>
// Finder cannot animate DMG artwork; the animated welcome plays on first launch.
// Geometry (points, origin top-left) must match xtask/src/dmg.rs.
import AppKit

let args = CommandLine.arguments
guard args.count == 4, let scale = Double(args[3]), let glyph = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write("usage: background.swift <glyph.png> <out.png> <scale>\n".data(using: .utf8)!)
    exit(2)
}
let width = 660.0, height = 400.0
let iconLeft = 170.0, iconRight = 490.0, iconY = 200.0, chipY = 266.0

guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
    let ctx = NSGraphicsContext(bitmapImageRep: rep)
else { exit(1) }
rep.size = NSSize(width: width, height: height)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = ctx
let xf = NSAffineTransform()
xf.scale(by: CGFloat(scale))
xf.concat()

func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255, green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: a)
}
func fy(_ y: Double) -> Double { height - y }
let white: UInt32 = 0xFFFFFF

// Black with a soft falloff behind the drag target.
NSGradient(starting: rgb(0x080808), ending: rgb(0x181818))!
    .draw(in: NSRect(x: 0, y: 0, width: width, height: height), angle: -35)
for (cx, cy, r, a) in [(width - 40, fy(30), 300.0, 0.10), (width / 2, fy(iconY), 230.0, 0.07)] {
    NSGradient(starting: rgb(white, a), ending: rgb(white, 0))!
        .draw(fromCenter: NSPoint(x: cx, y: cy), radius: 0, toCenter: NSPoint(x: cx, y: cy), radius: r, options: [])
}
// Hairline inner border.
let border = NSBezierPath(rect: NSRect(x: 0.5, y: 0.5, width: width - 1, height: height - 1))
rgb(0xFFFFFF, 0.06).setStroke(); border.lineWidth = 1; border.stroke()

// The official ribbon, drawn in white from its alpha silhouette.
let gs = 44.0
let tinted = NSImage(size: NSSize(width: gs, height: gs))
tinted.lockFocus()
glyph.draw(in: NSRect(x: 0, y: 0, width: gs, height: gs))
rgb(white).setFill()
NSRect(x: 0, y: 0, width: gs, height: gs).fill(using: .sourceIn)
tinted.unlockFocus()
tinted.draw(in: NSRect(x: 34, y: fy(30 + gs), width: gs, height: gs))

func text(_ s: String, _ x: Double, _ y: Double, size: CGFloat, weight: NSFont.Weight, color: NSColor, center: Bool = false) {
    let style = NSMutableParagraphStyle()
    style.alignment = center ? .center : .left
    let str = NSAttributedString(string: s, attributes: [
        .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color, .paragraphStyle: style,
    ])
    let h = str.size().height
    let w = center ? width : 560
    str.draw(in: NSRect(x: center ? 0 : x, y: fy(y) - h, width: w, height: h + 2))
}
text("Install Aeri", 90, 30, size: 24, weight: .semibold, color: rgb(0xF5F5F5))
text("Drag Aeri onto Applications.", 90, 61, size: 13, weight: .regular, color: rgb(0xFFFFFF, 0.62))

// Soft halos behind both icons.
for cx in [iconLeft, iconRight] {
    NSGradient(starting: rgb(white, 0.10), ending: rgb(white, 0))!
        .draw(fromCenter: NSPoint(x: cx, y: fy(iconY)), radius: 0,
              toCenter: NSPoint(x: cx, y: fy(iconY)), radius: 92, options: [])
}
// Finder labels can be light or dark. Neutral mid-gray works for either.
for cx in [iconLeft, iconRight] {
    let chip = NSBezierPath(roundedRect: NSRect(x: cx - 62, y: fy(chipY + 24), width: 124, height: 24),
                            xRadius: 12, yRadius: 12)
    rgb(0x757575).setFill(); chip.fill()
}

// Dashed directional arrow between the two icons.
let ax0 = iconLeft + 92, ax1 = iconRight - 92
let line = NSBezierPath()
line.move(to: NSPoint(x: ax0, y: fy(iconY)))
line.line(to: NSPoint(x: ax1 - 6, y: fy(iconY)))
line.lineWidth = 3; line.lineCapStyle = .round
line.setLineDash([2, 9], count: 2, phase: 0)
rgb(white, 0.72).setStroke(); line.stroke()
let head = NSBezierPath()
head.move(to: NSPoint(x: ax1 - 16, y: fy(iconY - 13)))
head.line(to: NSPoint(x: ax1, y: fy(iconY)))
head.line(to: NSPoint(x: ax1 - 16, y: fy(iconY + 13)))
head.lineWidth = 4; head.lineCapStyle = .round; head.lineJoinStyle = .round
rgb(white).setStroke(); head.stroke()

text("Then open Aeri from Applications. You can eject this disk image afterwards.",
     0, 352, size: 12, weight: .regular, color: rgb(0xFFFFFF, 0.62), center: true)

NSGraphicsContext.restoreGraphicsState()
guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
try png.write(to: URL(fileURLWithPath: args[2]))
