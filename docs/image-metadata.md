# Image task: privacy metadata stripping

Image tasks copy the validated file into a work directory, then run **exiftool** with a fixed set of arguments to remove **location- and GPS-related** tags. The source file on disk is never modified.

Keep this list in sync with `EXIFTOOL_PRIVACY_ARGS` in `src/processor/image/privacy_metadata_step.rs`.

## exiftool flags

| Flag | Purpose |
| --- | --- |
| `-m` | Ignore minor errors (e.g. missing tags on some formats) |
| `-q` | Quiet |
| `-overwrite_original` | Write changes to the working copy (no `*_original` backup) |
| `-P` | Preserve file modification time |

## Tags cleared (empty `=` deletes the tag)

- `-gps:all=` — all GPS-related EXIF tags exiftool groups under GPS
- `-XMP:GPSLatitude=` `-XMP:GPSLongitude=` `-XMP:GPSAltitude=`
- `-XMP-iptcCore:City=` `-XMP-iptcCore:CountryName=` `-XMP-iptcCore:Location=` `-XMP-iptcCore:Region=`
- `-IPTC:City=` `-IPTC:Sub-location=` `-IPTC:Province-State=` `-IPTC:Country-PrimaryLocationName=` `-IPTC:Country-PrimaryLocationCode=`

Other camera metadata (e.g. lens, exposure) may remain unless you add a future task option for a full strip.

## Tooling

**exiftool** must be on `PATH`. See [Getting started](getting-started.md).
