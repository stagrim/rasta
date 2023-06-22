// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { ImageData } from "./ImageData";
import type { TextData } from "./TextData";
import type { WebsiteData } from "./WebsiteData";

export type PlaylistItem = { type: "WEBSITE", name: string, settings: WebsiteData, } | { type: "TEXT", name: string, settings: TextData, } | { type: "IMAGE", name: string, settings: ImageData, };