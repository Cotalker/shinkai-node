import { EncryptionMethod, ShinkaiMessageBuilderWrapper, ShinkaiMessageWrapper as ShinkaiMessageWrapperWASM } from '../../pkg/shinkai_message_wasm.js';
import { Body, ExternalMetadata, ShinkaiMessage } from '../../models/ShinkaiMessage';
import { mapEncryptionMethod } from '../../utils/wasm_helpers.js';

export class ShinkaiMessageWrapper {
  private wasmWrapper: ShinkaiMessageWrapperWASM;

  constructor(message: ShinkaiMessage) {
    this.wasmWrapper = new ShinkaiMessageWrapperWASM(message.body, message.external_metadata, mapEncryptionMethod(message.encryption));
  }

  static fromJsValue(j: any): ShinkaiMessageWrapper {
    const message: ShinkaiMessage = j;
    return new ShinkaiMessageWrapper(message);
  }

  to_jsvalue(): any {
    return this.wasmWrapper.to_jsvalue();
  }

  to_json_str(): string {
    return this.wasmWrapper.to_json_str();
  }

  calculate_hash(): string {
    return this.wasmWrapper.calculate_hash();
  }

  static time_now(): string {
    return ShinkaiMessageWrapperWASM.generate_time_now();
  }

  static from_json_str(s: string): ShinkaiMessageWrapper {
    const message: ShinkaiMessage = JSON.parse(s);
    return new ShinkaiMessageWrapper(message);
  }

  get body(): Body {
    return this.wasmWrapper.body;
  }

  get encryption(): string {
    return this.wasmWrapper.encryption;
  }

  get external_metadata(): ExternalMetadata {
    return this.wasmWrapper.external_metadata;
  }
}
