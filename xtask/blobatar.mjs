// Run in a scratch directory with blobatar@2.7.0 and @resvg/resvg-js@2.6.2.
import { Buffer } from "node:buffer";
import { writeFileSync } from "node:fs";
import { Resvg } from "@resvg/resvg-js";
import { blobatar } from "blobatar";

const output = process.argv[2];
if (!output) throw new Error("Usage: bun blobatar.mjs OUTPUT");

const faces = Array.from({ length: 64 }, (_, index) => {
  const seed = `aeri-profile-${String(index).padStart(2, "0")}`;
  return new Resvg(blobatar(seed, { size: 80, background: "circle" })).render().asPng();
});

// Little-endian byte offsets, then concatenated PNGs. An avatar can be
// decoded individually without allocating or retaining an entire atlas.
const offsets = Buffer.alloc((faces.length + 1) * 4);
let offset = offsets.length;
faces.forEach((png, index) => {
  offsets.writeUInt32LE(offset, index * 4);
  offset += png.length;
});
offsets.writeUInt32LE(offset, faces.length * 4);
writeFileSync(output, Buffer.concat([offsets, ...faces]));
