// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_UTIL_TEXTURE_H
#define WHELK_UTIL_TEXTURE_H

#include <gdk-pixbuf/gdk-pixbuf.h>
#include <gdk/gdk.h>

G_BEGIN_DECLS

/* =========================================================================
 * A PIXBUF AS A TEXTURE
 * =========================================================================
 * GdkPixbuf is the loader with the format plugins; GdkTexture is what GTK
 * draws.  GTK 4.20 deprecated gdk_texture_new_for_pixbuf(), so this converts
 * the pixels explicitly: tightly packed RGBA, premultiplied, which is the one
 * layout both libraries agree on without a PNG round trip.
 *
 * An opaque picture is copied as is.  An alpha channel is premultiplied, and a
 * fully transparent pixel is written as transparent black rather than as
 * whatever colour happened to sit under it.
 */

GdkTexture *whelk_texture_from_pixbuf(GdkPixbuf *pixbuf);

G_END_DECLS

#endif // WHELK_UTIL_TEXTURE_H
