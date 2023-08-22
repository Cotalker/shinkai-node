import SHA256 from "crypto-js/sha256";
import { ShinkaiMessage } from "../models/ShinkaiMessage";
import { ShinkaiMessageWrapper } from "../lib/wasm/ShinkaiMessageWrapper";

export function calculateMessageHash(message: ShinkaiMessage): string {
  const messageWrapper = new ShinkaiMessageWrapper(message);
  return messageWrapper.calculate_hash();
}
