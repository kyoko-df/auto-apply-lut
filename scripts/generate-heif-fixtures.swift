// Generated test images only. Run: xcrun swift scripts/generate-heif-fixtures.swift /tmp/lutlab-format-fixtures
import Foundation
import CoreGraphics
import ImageIO
let root = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
let width = 64, height = 48
var rgba = [UInt16](repeating: 65535, count: width * height * 4)
for y in 0..<height {
    for x in 0..<width {
        let i = (y * width + x) * 4
        // Asymmetric quadrants identify all eight EXIF transforms and vertical flips.
        rgba[i] = x < width / 2 ? 52000 : 7000
        rgba[i+1] = y < height / 2 ? 12000 : 53000
        rgba[i+2] = UInt16(1000 + x * 700 + y * 100)
    }
}
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
try rgba.withUnsafeMutableBytes { bytes in
    let context = CGContext(data:bytes.baseAddress, width:width, height:height, bitsPerComponent:16, bytesPerRow:width*8,
        space:cs, bitmapInfo:CGBitmapInfo.byteOrder16Little.rawValue | CGImageAlphaInfo.noneSkipLast.rawValue)!
    let image = context.makeImage()!
    for orientation in 1...8 {
        let url = root.appendingPathComponent("generated-orientation-\(orientation).heic")
        if FileManager.default.fileExists(atPath:url.path) { throw NSError(domain:"protect-existing-fixture", code:1) }
        let dest = CGImageDestinationCreateWithURL(url as CFURL, "public.heic" as CFString, 1, nil)!
        CGImageDestinationAddImage(dest, image, [kCGImagePropertyOrientation:orientation, kCGImageDestinationLossyCompressionQuality:1.0] as CFDictionary)
        if !CGImageDestinationFinalize(dest) { throw NSError(domain:"HEIF-encoder", code:1) }
    }
}
print("Generated 8 full-image HEIF orientation fixtures in", root.path)
