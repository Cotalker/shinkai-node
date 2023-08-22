import {
  isTool,
  isString,
  isOptional,
  isEnum,
  isInput,
  isOutput,
  description,
  isJSON,
  BaseTool,
  BaseInput,
  BaseOutput,
} from '@shinkai/toolkit-lib';
import axios from 'axios';

@isInput('HTTP')
class HTTPInput extends BaseInput {
  @description('URL to fetch')
  url!: string;

  @isEnum(['get', 'post', 'put', 'delete'], 'HTTP method to use')
  @isOptional
  method: 'get' | 'post' | 'put' | 'delete' = 'get';

  @isJSON('HTTP headers to send')
  @isOptional
  headers: Record<string, string> = {};

  @isJSON('HTTP body to send')
  @isOptional
  data!: Record<string, string>;
}

@isOutput('HTTP')
class HTTPOutput extends BaseOutput {
  @isString('Response body')
  response!: string;
}

@isTool
export class HTTP extends BaseTool<HTTPInput, HTTPOutput> {
  description = 'Fetch content from URL';

  async run(input: HTTPInput): Promise<HTTPOutput> {
    const config = {
      method: input.method,
      url: input.url,
      headers: input.headers,
      data: input.data,
    };
    const response = await axios(config);

    const out = new HTTPOutput();
    out.response = response.data;

    return out;
  }
}
