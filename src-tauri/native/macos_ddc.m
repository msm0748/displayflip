// IOAVService transport discovery is adapted from waydabber/m1ddc (MIT).
// See LICENSE-m1ddc.txt. CoreGraphics enumeration is independent of DDC support.
#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <ColorSync/ColorSync.h>
#import <IOKit/IOKitLib.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>

typedef CFTypeRef (*CreateAVService)(CFAllocatorRef, io_service_t);
typedef IOReturn (*AVTransfer)(CFTypeRef, uint32_t, uint32_t, void *, uint32_t);
typedef CFDictionaryRef (*CreateDisplayInfo)(CGDirectDisplayID);

static CreateDisplayInfo displayInfoFunction(void) {
    static CreateDisplayInfo function;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        void *library = dlopen("/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay", RTLD_LAZY);
        if (library) function = (CreateDisplayInfo)dlsym(library, "CoreDisplay_DisplayCreateInfoDictionary");
    });
    return function;
}

static void *ioKitLibrary(void) {
    static void *library;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        library = dlopen("/System/Library/Frameworks/IOKit.framework/IOKit", RTLD_LAZY);
    });
    return library;
}

// The caller owns this malloc allocation and releases it with df_macos_free.
char *df_macos_displays(int32_t *status) {
    @autoreleasepool {
        uint32_t count = 0;
        *status = CGGetOnlineDisplayList(0, NULL, &count);
        if (*status != kCGErrorSuccess) return NULL;
        CGDirectDisplayID *ids = calloc(count ?: 1, sizeof(CGDirectDisplayID));
        if (!ids) { *status = kCGErrorFailure; return NULL; }
        *status = CGGetOnlineDisplayList(count, ids, &count);
        if (*status != kCGErrorSuccess) { free(ids); return NULL; }
        NSMutableArray *displays = [NSMutableArray array];
        CreateDisplayInfo infoFunction = displayInfoFunction();
        for (uint32_t i = 0; i < count; i++) {
            if (CGDisplayIsBuiltin(ids[i])) continue;
            CFUUIDRef uuidRef = CGDisplayCreateUUIDFromDisplayID(ids[i]);
            if (!uuidRef) continue;
            NSString *uuid = CFBridgingRelease(CFUUIDCreateString(NULL, uuidRef));
            CFRelease(uuidRef);
            NSDictionary *info = infoFunction ? CFBridgingRelease(infoFunction(ids[i])) : nil;
            NSDictionary *names = info[@"DisplayProductName"];
            NSString *name = [names isKindOfClass:[NSDictionary class]] ? (names[@"en_US"] ?: names.allValues.firstObject) : nil;
            if (![name isKindOfClass:[NSString class]] || !name.length) name = @"외부 모니터";
            NSString *location = info[@"IODisplayLocation"];
            if (![location isKindOfClass:[NSString class]]) location = @"";
            [displays addObject:@{@"id": uuid, @"name": name, @"location": location}];
        }
        free(ids);
        NSData *json = [NSJSONSerialization dataWithJSONObject:displays options:0 error:nil];
        if (!json) { *status = kCGErrorFailure; return NULL; }
        char *result = calloc(json.length + 1, 1);
        if (!result) { *status = kCGErrorFailure; return NULL; }
        memcpy(result, json.bytes, json.length);
        return result;
    }
}

void df_macos_free(char *value) { free(value); }

static bool isMCDPTransport(io_service_t proxy) {
    io_registry_entry_t parent = IO_OBJECT_NULL;
    if (IORegistryEntryGetParentEntry(proxy, kIOServicePlane, &parent) != KERN_SUCCESS) return false;
    CFTypeRef provider = IORegistryEntryCreateCFProperty(parent, CFSTR("EPICProviderClass"), NULL, 0);
    bool matches = provider && CFGetTypeID(provider) == CFStringGetTypeID()
        && CFEqual(provider, CFSTR("AppleDCPMCDP29XX"));
    if (provider) CFRelease(provider);
    IOObjectRelease(parent);
    return matches;
}

// Resolve the exact IORegistry framebuffer supplied by CoreDisplay. Matching by
// vendor/model is unsafe for two identical monitors, especially with serial 0.
CFTypeRef df_macos_open(const char *location, uint32_t *chipAddress) {
#if defined(__aarch64__)
    if (!location || !*location || !ioKitLibrary()) return NULL;
    CreateAVService create = (CreateAVService)dlsym(ioKitLibrary(), "IOAVServiceCreateWithService");
    if (!create) return NULL;
    @autoreleasepool {
        NSString *path = [NSString stringWithUTF8String:location];
        // MACH_PORT_NULL selects the default IOKit port on all supported macOS
        // versions; kIOMainPortDefault itself is unavailable before macOS 12.
        io_registry_entry_t adapter = IORegistryEntryCopyFromPath(MACH_PORT_NULL, (__bridge CFStringRef)path);
        if (!adapter) return NULL;
        uint64_t adapterID = 0;
        kern_return_t status = IORegistryEntryGetRegistryEntryID(adapter, &adapterID);
        IOObjectRelease(adapter);
        if (status != KERN_SUCCESS) return NULL;
        io_registry_entry_t root = IORegistryGetRootEntry(MACH_PORT_NULL);
        io_iterator_t iterator = IO_OBJECT_NULL;
        status = IORegistryEntryCreateIterator(root, kIOServicePlane, kIORegistryIterateRecursively, &iterator);
        IOObjectRelease(root);
        if (status != KERN_SUCCESS) return NULL;
        bool selectedFramebuffer = false;
        CFTypeRef result = NULL;
        io_service_t entry;
        while ((entry = IOIteratorNext(iterator))) {
            if (IOObjectConformsTo(entry, "IOMobileFramebuffer")) {
                uint64_t entryID = 0;
                selectedFramebuffer = IORegistryEntryGetRegistryEntryID(entry, &entryID) == KERN_SUCCESS
                    && entryID == adapterID;
            } else if (selectedFramebuffer && IOObjectConformsTo(entry, "DCPAVServiceProxy")) {
                CFTypeRef external = IORegistryEntryCreateCFProperty(entry, CFSTR("Location"), NULL, 0);
                if (external && CFEqual(external, CFSTR("External"))) {
                    result = create(NULL, entry);
                    if (result) *chipAddress = isMCDPTransport(entry) ? 0xb7 : 0x37;
                }
                if (external) CFRelease(external);
            }
            IOObjectRelease(entry);
            if (result) break;
        }
        IOObjectRelease(iterator);
        return result;
    }
#else
    (void)location; (void)chipAddress;
    return NULL;
#endif
}

int32_t df_macos_write(CFTypeRef service, uint32_t chipAddress, const uint8_t *packet, uint32_t length) {
    if (!service || !ioKitLibrary()) return kIOReturnUnsupported;
    AVTransfer write = (AVTransfer)dlsym(ioKitLibrary(), "IOAVServiceWriteI2C");
    return write ? write(service, chipAddress, 0x51, (void *)packet, length) : kIOReturnUnsupported;
}

int32_t df_macos_read(CFTypeRef service, uint32_t chipAddress, uint8_t *reply, uint32_t length) {
    if (!service || !ioKitLibrary()) return kIOReturnUnsupported;
    AVTransfer read = (AVTransfer)dlsym(ioKitLibrary(), "IOAVServiceReadI2C");
    return read ? read(service, chipAddress, 0x51, reply, length) : kIOReturnUnsupported;
}

void df_macos_close(CFTypeRef service) { if (service) CFRelease(service); }
