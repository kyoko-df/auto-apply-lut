// App-owned C ABI bridge to the verified external LibRaw 0.21.4 static library.
// Never calls unpack_thumb/dcraw_make_mem_thumb or any JPEG preview API.
#include "libraw/libraw.h"
#include <cstdint>
#include <ctime>
#include <cstdio>
#include <cstring>
#include <exception>
#include <cmath>
struct RawInfo {
    uint32_t width, height, orientation;
    float iso, shutter, aperture, focal;
    char date_time[20];
    char make[64], model[64], lens[128];
};
struct Cancel { int (*check)(void *); void *context; };
static int progress(void *p, enum LibRaw_progress, int, int) {
    auto c = static_cast<Cancel *>(p); return c->check(c->context);
}
extern "C" const char *lutlab_raw_version() { return LibRaw::version(); }
extern "C" int lutlab_raw(const uint8_t *data, size_t length, uint16_t *rgb, size_t samples,
                          float exposure_ev, int white_balance,
                          RawInfo *info, int (*check)(void *), void *context, char *error, size_t error_size) {
    auto fail = [&](const char *s) { std::snprintf(error, error_size, "%s", s); return 0; };
    try {
        if (!std::isfinite(exposure_ev) || exposure_ev < -2.0f || exposure_ev > 3.0f ||
            white_balance < 0 || white_balance > 2) return fail("Invalid RAW development settings");
        LibRaw raw; Cancel cancel{check, context}; raw.set_progress_handler(progress, &cancel);
        raw.imgdata.rawparams.max_raw_memory_mb = 1024;
        int status = raw.open_buffer(data, length);
        if (status) return fail(LibRaw::strerror(status));
        auto &d = raw.imgdata;
        if (std::strcmp(d.idata.make, "Sony") && std::strcmp(d.idata.make, "SONY"))
            return fail("Input is not a Sony sensor RAW");
        if (d.idata.raw_count != 1 || !d.idata.filters) return fail("Only single-frame CFA sensor RAW is supported");
        if (d.sizes.pixel_aspect != 1.0) return fail("Non-square RAW pixels are unsupported");
        static const uint32_t orientations[8] = {1,2,4,3,5,8,6,7};
        if (d.sizes.flip < 0 || d.sizes.flip > 7) return fail("Invalid RAW orientation");
        // Promote the standard active-image crop, never the optional user/aspect crop.
        const auto &crop = d.sizes.raw_inset_crops[0];
        if (crop.cwidth && crop.cheight) {
            if (crop.cleft == 65535 || crop.ctop == 65535 ||
                uint64_t(crop.cleft) + crop.cwidth > d.sizes.raw_width ||
                uint64_t(crop.ctop) + crop.cheight > d.sizes.raw_height ||
                raw.adjust_to_raw_inset_crop(1, 0.0f) != 1) return fail("Invalid Sony active-image crop");
        }
        info->width = d.sizes.width; info->height = d.sizes.height;
        info->orientation = orientations[d.sizes.flip];
        info->iso = d.other.iso_speed; info->shutter = d.other.shutter;
        info->aperture = d.other.aperture; info->focal = d.other.focal_len;
        // LibRaw parses the timezone-less EXIF clock using mktime. Reverse it
        // using localtime, not UTC, so changing the computer TZ does not shift it.
        std::tm clock{};
        if (d.other.timestamp > 0) {
#ifdef _WIN32
            if (localtime_s(&clock, &d.other.timestamp) == 0)
#else
            if (localtime_r(&d.other.timestamp, &clock))
#endif
                std::strftime(info->date_time, sizeof(info->date_time), "%Y:%m:%d %H:%M:%S", &clock);
        }
        std::snprintf(info->make, sizeof(info->make), "%s", d.idata.make);
        std::snprintf(info->model, sizeof(info->model), "%s", d.idata.model);
        std::snprintf(info->lens, sizeof(info->lens), "%s", d.lens.Lens);
        if (!info->width || !info->height || info->width > 32768 || info->height > 32768 ||
            uint64_t(info->width) * info->height > 64000000) return fail("RAW exceeds 64 MP / 32768 side limit");
        if (!rgb) return 1;
        if (samples != size_t(info->width) * info->height * 3) return fail("RAW buffer size mismatch");
        // Develop full sensor data into LINEAR sRGB primaries. LittleCMS applies
        // the actual sRGB transfer curve afterwards, consistently on all OSes.
        d.params.half_size = 0; d.params.four_color_rgb = 0; d.params.user_flip = 0;
        d.params.output_bps = 16; d.params.output_color = 1;
        d.params.gamm[0] = 1.0; d.params.gamm[1] = 1.0;
        d.params.use_camera_wb = white_balance == 0;
        d.params.use_auto_wb = white_balance == 2;
        d.params.no_auto_bright = 1; d.params.adjust_maximum_thr = 0;
        d.params.bright = 1.0; d.params.highlight = 0; d.params.user_qual = 3;
        d.params.exp_correc = exposure_ev != 0.0f;
        d.params.exp_shift = std::exp2(exposure_ev);
        d.params.exp_preser = 0.0f;
        if ((status = raw.unpack()) || (status = raw.dcraw_process())) return fail(LibRaw::strerror(status));
        libraw_processed_image_t *image = raw.dcraw_make_mem_image(&status);
        if (!image) return fail(LibRaw::strerror(status));
        const bool valid = image->type == LIBRAW_IMAGE_BITMAP && image->bits == 16 && image->colors == 3 &&
            image->width == info->width && image->height == info->height && image->data_size == samples * 2;
        if (valid) std::memcpy(rgb, image->data, image->data_size);
        LibRaw::dcraw_clear_mem(image);
        if (!valid) return fail("LibRaw did not return the full RGB16 sensor image");
        return check(context) ? fail("RAW development cancelled") : 1;
    } catch (const std::exception &e) { return fail(e.what()); }
      catch (...) { return fail("LibRaw development failed"); }
}
