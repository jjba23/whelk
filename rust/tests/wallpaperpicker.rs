// The wallpaper picker's policy, tested without a display: the names a tile
// wears, the count it announces, the thumbnail and card sizes, and the Scheme
// string literal a saved preference is written with.

use whelk_rs::guile::scheme_string_literal;
use whelk_rs::wallpaperpicker;

#[test]
fn a_tile_is_named_after_the_file_without_its_extension() {
    assert_eq!(
        wallpaperpicker::tile_name("/home/me/Pictures/Sunset.4k.jpg"),
        "Sunset.4k"
    );
    assert_eq!(wallpaperpicker::tile_name("/walls/plain"), "plain");
    assert_eq!(wallpaperpicker::tile_name("bare.png"), "bare");
    assert_eq!(wallpaperpicker::tile_name("/walls/.hidden"), ".hidden");
}

#[test]
fn the_header_counts_what_it_found() {
    assert_eq!(wallpaperpicker::summary(0), "0 wallpapers");
    assert_eq!(wallpaperpicker::summary(1), "1 wallpaper");
    assert_eq!(wallpaperpicker::summary(7), "7 wallpapers");
}

#[test]
fn thumbnails_divide_the_card_between_the_columns() {
    // 960 wide, four columns, 32 px of padding a side and 12 px between them.
    assert_eq!(wallpaperpicker::thumb_size(960, 4), (215, 120));
    // A narrow card still gets a usable thumbnail.
    assert_eq!(wallpaperpicker::thumb_size(320, 12), (96, 72));
    // A card with no columns named is one column, not a division by zero.
    assert_eq!(wallpaperpicker::thumb_size(400, 0), (336, 189));
}

#[test]
fn the_card_is_the_configured_size_until_it_expands() {
    assert_eq!(
        wallpaperpicker::card_size(false, (960, 640), Some((3840, 2160))),
        (960, 640)
    );
    assert_eq!(
        wallpaperpicker::card_size(true, (960, 640), Some((3840, 2160))),
        (3744, 2064)
    );
}

#[test]
fn a_small_monitor_caps_the_card() {
    assert_eq!(
        wallpaperpicker::card_size(false, (960, 640), Some((800, 600))),
        (704, 504)
    );
    // No monitor model: the configured size stands, as the C picker leaves it.
    assert_eq!(
        wallpaperpicker::card_size(false, (960, 640), None),
        (960, 640)
    );
}

#[test]
fn a_saved_path_is_quoted_for_scheme() {
    assert_eq!(
        scheme_string_literal("/home/me/walls/plain.jpg"),
        "\"/home/me/walls/plain.jpg\""
    );
    assert_eq!(
        scheme_string_literal("/home/me/a \"quoted\" name.jpg"),
        "\"/home/me/a \\\"quoted\\\" name.jpg\""
    );
    assert_eq!(
        scheme_string_literal("/home/me/back\\slash.jpg"),
        "\"/home/me/back\\\\slash.jpg\""
    );
}
