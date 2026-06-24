// macOS native PDF render + OCR using PDFKit + Vision
// Usage: swift pdf-ocr.swift <input.pdf> [output.json]
//
// Renders each page to a CGImage via PDFKit (which handles corrupt PDFs better
// than poppler), then runs Vision VNRecognizeTextRequest for OCR.
// This is the same pipeline that Preview uses for "Select All" + "Copy as Text".

import Foundation
import PDFKit
import Vision
import AppKit

guard CommandLine.arguments.count > 1 else {
    print("Usage: swift pdf-ocr.swift <input.pdf> [output.json]")
    exit(1)
}

let inputPath = CommandLine.arguments[1]
let outputPath = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : nil

guard let pdfDoc = PDFDocument(url: URL(fileURLWithPath: inputPath)) else {
    let result: [String: Any] = [
        "ok": false,
        "error_code": "pdfkit_open_failed",
        "error_message": "PDFKit could not open the file (even with its recovery logic)",
        "page_count": 0,
    ]
    writeOutput(result, to: outputPath)
    exit(1)
}

let pageCount = pdfDoc.pageCount
var allText: [String] = []
var perPageResults: [[String: Any]] = []

for i in 0..<pageCount {
    guard let page = pdfDoc.page(at: i) else { continue }
    let bounds = page.bounds(for: .mediaBox)

    // Render page to NSImage at 200 DPI
    let dpi: CGFloat = 200.0
    let scale = dpi / 72.0  // PDF points -> 200 DPI
    let pixelSize = NSSize(width: bounds.width * scale, height: bounds.height * scale)

    let img = NSImage(size: pixelSize)
    img.lockFocus()
    if let ctx = NSGraphicsContext.current?.cgContext {
        ctx.saveGState()
        ctx.setFillColor(NSColor.white.cgColor)
        ctx.fill(CGRect(origin: .zero, size: pixelSize))
        ctx.scaleBy(x: scale, y: scale)
        // PDFKit draws in PDF coords (origin bottom-left), AppKit in screen coords (origin bottom-left too)
        page.draw(with: .mediaBox, to: ctx)
        ctx.restoreGState()
    }
    img.unlockFocus()

    guard let tiffData = img.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiffData),
          let cgImage = bitmap.cgImage else {
        perPageResults.append(["page": i+1, "ok": false, "error": "render_failed"])
        continue
    }

    // Vision OCR
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true
    request.recognitionLanguages = ["th-TH", "en-US"]

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    do {
        try handler.perform([request])
    } catch {
        perPageResults.append(["page": i+1, "ok": false, "error": "vision_failed: \(error)"])
        continue
    }

    let observations = request.results ?? []
    var pageText: [String] = []
    for obs in observations {
        if let top = obs.topCandidates(1).first {
            pageText.append(top.string)
        }
    }
    let joined = pageText.joined(separator: "\n")
    allText.append(joined)
    perPageResults.append([
        "page": i+1,
        "ok": true,
        "chars": joined.count,
        "lines": pageText.count,
    ])
}

let fullText = allText.joined(separator: "\n\n")
let result: [String: Any] = [
    "ok": true,
    "text": fullText,
    "page_count": pageCount,
    "method": "pdfkit_vision",
    "pages": perPageResults,
    "text_length": fullText.count,
]

writeOutput(result, to: outputPath)
print("OK: \(pageCount) page(s), \(fullText.count) chars extracted")

func writeOutput(_ dict: [String: Any], to path: String?) {
    let data: Data
    do {
        data = try JSONSerialization.data(withJSONObject: dict, options: [.prettyPrinted, .sortedKeys])
    } catch {
        FileHandle.standardError.write("JSON serialize failed: \(error)\n".data(using: .utf8)!)
        return
    }
    if let path = path {
        do { try data.write(to: URL(fileURLWithPath: path)) }
        catch { FileHandle.standardError.write("Write to \(path) failed: \(error)\n".data(using: .utf8)!) }
    } else {
        FileHandle.standardOutput.write(data)
    }
}
