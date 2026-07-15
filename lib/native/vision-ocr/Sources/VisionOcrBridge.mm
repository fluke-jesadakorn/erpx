// VisionOcrBridge.mm
// N-API addon: macOS Vision OCR, pure Objective-C++ (no Swift).
// Exposes two JS functions:
//   ocrPixels({ pixels: Buffer, width, height, languages?: string[] })
//     → { ok, lines: string[], error_code? }
//   ocrImageFile({ data: Buffer })
//     → { ok, lines: string[], error_code? }

#import <napi.h>
#import <Foundation/Foundation.h>
#import <Vision/Vision.h>
#import <CoreGraphics/CoreGraphics.h>
#import <AppKit/AppKit.h>

static NSArray *RecognizeWithCGImage(CGImageRef cgImage, NSArray *languages) {
    if (!cgImage) return @[];
    VNRecognizeTextRequest *request = [[VNRecognizeTextRequest alloc] init];
    request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
    request.usesLanguageCorrection = YES;
    request.recognitionLanguages = languages ?: @[@"th-TH", @"en-US"];

    VNImageRequestHandler *handler =
        [[VNImageRequestHandler alloc] initWithCGImage:cgImage options:@{}];
    NSError *err = nil;
    if (![handler performRequests:@[request] error:&err]) {
        return @[];
    }
    NSMutableArray *out = [NSMutableArray array];
    for (VNRecognizedTextObservation *obs in (request.results ?: @[])) {
        VNRecognizedText *top = [obs topCandidates:1].firstObject;
        if (top && top.string) [out addObject:top.string];
    }
    return out;
}

static Napi::Array NSArrayToJs(Napi::Env env, NSArray *arr) {
    Napi::Array js = Napi::Array::New(env, arr.count);
    for (NSUInteger i = 0; i < arr.count; i++) {
        id v = arr[i];
        if ([v isKindOfClass:[NSString class]]) {
            js.Set((uint32_t)i, Napi::String::New(env, [(NSString *)v UTF8String]));
        }
    }
    return js;
}

Napi::Value OcrPixels(const Napi::CallbackInfo &info) {
    Napi::Env env = info.Env();
    if (info.Length() < 1 || !info[0].IsObject()) {
        Napi::TypeError::New(env, "ocrPixels expects { pixels, width, height, languages? }").ThrowAsJavaScriptException();
        return env.Null();
    }
    Napi::Object obj = info[0].As<Napi::Object>();
    if (!obj.Has("pixels") || !obj.Has("width") || !obj.Has("height")) {
        Napi::TypeError::New(env, "ocrPixels requires pixels, width, height").ThrowAsJavaScriptException();
        return env.Null();
    }

    Napi::Buffer<char> buf = obj.Get("pixels").As<Napi::Buffer<char>>();
    int32_t width  = obj.Get("width").ToNumber().Int32Value();
    int32_t height = obj.Get("height").ToNumber().Int32Value();

    NSMutableArray *langs = [NSMutableArray arrayWithObjects:@"th-TH", @"en-US", nil];
    if (obj.Has("languages") && obj.Get("languages").IsArray()) {
        Napi::Array arr = obj.Get("languages").As<Napi::Array>();
        [langs removeAllObjects];
        for (uint32_t i = 0; i < arr.Length(); i++) {
            Napi::Value v = arr.Get(i);
            if (v.IsString()) {
                [langs addObject:[NSString stringWithUTF8String:v.ToString().Utf8Value().c_str()]];
            }
        }
    }

    if (width <= 0 || height <= 0 || buf.Length() == 0) {
        Napi::Object out = Napi::Object::New(env);
        out.Set("ok", Napi::Boolean::New(env, false));
        out.Set("lines", Napi::Array::New(env, 0));
        out.Set("error_code", Napi::String::New(env, "invalid_input"));
        return out;
    }

    NSData *rgba = [NSData dataWithBytes:buf.Data() length:buf.Length()];
    CGDataProviderRef provider = CGDataProviderCreateWithCFData((__bridge CFDataRef)rgba);
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    CGBitmapInfo bmpInfo = (CGBitmapInfo)kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big;
    CGImageRef cg = CGImageCreate((size_t)width,
                                  (size_t)height,
                                  8, 32,
                                  (size_t)width * 4,
                                  cs, bmpInfo,
                                  provider, NULL,
                                  true, kCGRenderingIntentDefault);

    NSArray *lines = RecognizeWithCGImage(cg, langs);

    if (cg) CGImageRelease(cg);
    CGColorSpaceRelease(cs);
    CGDataProviderRelease(provider);

    Napi::Object out = Napi::Object::New(env);
    out.Set("ok", Napi::Boolean::New(env, true));
    out.Set("lines", NSArrayToJs(env, lines));
    return out;
}

Napi::Value OcrImageFile(const Napi::CallbackInfo &info) {
    Napi::Env env = info.Env();
    if (info.Length() < 1 || !info[0].IsObject()) {
        Napi::TypeError::New(env, "ocrImageFile expects { data }").ThrowAsJavaScriptException();
        return env.Null();
    }
    Napi::Object obj = info[0].As<Napi::Object>();
    if (!obj.Has("data") || !obj.Get("data").IsBuffer()) {
        Napi::TypeError::New(env, "ocrImageFile requires data: Buffer").ThrowAsJavaScriptException();
        return env.Null();
    }
    Napi::Buffer<char> buf = obj.Get("data").As<Napi::Buffer<char>>();
    NSData *data = [NSData dataWithBytes:buf.Data() length:buf.Length()];
    NSImage *img = [[NSImage alloc] initWithData:data];
    CGImageRef cg = NULL;
    if (img) {
        NSData *tiff = [img TIFFRepresentation];
        if (tiff) {
            NSBitmapImageRep *rep = [NSBitmapImageRep imageRepWithData:tiff];
            cg = [rep CGImage];
        }
    }
    NSArray *lines = RecognizeWithCGImage(cg, @[@"th-TH", @"en-US"]);
    if (cg) CGImageRelease(cg);

    Napi::Object out = Napi::Object::New(env);
    out.Set("ok", Napi::Boolean::New(env, true));
    out.Set("lines", NSArrayToJs(env, lines));
    return out;
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
    exports.Set("ocrPixels", Napi::Function::New(env, OcrPixels));
    exports.Set("ocrImageFile", Napi::Function::New(env, OcrImageFile));
    exports.Set("platform", Napi::String::New(env, "darwin-vision"));
    return exports;
}

NODE_API_MODULE(vision_ocr, Init)