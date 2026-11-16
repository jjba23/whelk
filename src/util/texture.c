// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/texture.h"

GdkTexture *whelk_texture_from_pixbuf(GdkPixbuf *pixbuf) {
  if (!pixbuf)
    return NULL;

  const int width = gdk_pixbuf_get_width(pixbuf);
  const int height = gdk_pixbuf_get_height(pixbuf);
  const int channels = gdk_pixbuf_get_n_channels(pixbuf);
  const int rowstride = gdk_pixbuf_get_rowstride(pixbuf);
  const gboolean has_alpha = gdk_pixbuf_get_has_alpha(pixbuf);
  const guchar *pixels = gdk_pixbuf_get_pixels(pixbuf);

  /* Every loader in the stack produces RGB or RGBA; a 1/2-channel picture
   * would make the channel arithmetic below read past the row. */
  if (width <= 0 || height <= 0 || channels < 3 || !pixels)
    return NULL;

  const gsize stride = (gsize)width * 4;
  guchar *data = g_malloc_n(stride, (gsize)height);

  for (int y = 0; y < height; y++) {
    const guchar *source = pixels + (gsize)y * (gsize)rowstride;
    guchar *target = data + (gsize)y * stride;

    for (int x = 0; x < width; x++) {
      const guint alpha = has_alpha ? source[3] : 255;
      guint red = source[0];
      guint green = source[1];
      guint blue = source[2];

      if (alpha == 0) {
        red = 0;
        green = 0;
        blue = 0;
      } else if (alpha != 255) {
        red = (red * alpha + 127) / 255;
        green = (green * alpha + 127) / 255;
        blue = (blue * alpha + 127) / 255;
      }

      target[0] = (guchar)red;
      target[1] = (guchar)green;
      target[2] = (guchar)blue;
      target[3] = (guchar)alpha;

      source += channels;
      target += 4;
    }
  }

  g_autoptr(GBytes) bytes =
      g_bytes_new_take(data, stride * (gsize)height);
  GdkTexture *texture = gdk_memory_texture_new(
      width, height, GDK_MEMORY_R8G8B8A8_PREMULTIPLIED, bytes, stride);

  return texture;
}
