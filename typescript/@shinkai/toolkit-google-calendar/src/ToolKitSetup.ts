import {
  DATA_TYPES,
  SHINKAI_OAUTH,
  ShinkaiSetup,
  isToolKit,
} from '@shinkai/toolkit-lib';
import { googleCalendar } from './lib/google-calendar/src';
import axios from 'axios';

@isToolKit
export class ToolKitSetup extends ShinkaiSetup {
  'toolkit-name' = '@shinkai/toolkit-google-calendar';
  author = 'shinkai-dev';
  version = '0.0.1';

  toolkitHeaders = [
    // Register OAuth
    {
      name: SHINKAI_OAUTH,
      oauth: Object.assign({}, googleCalendar.auth, {
        cloudOAuth: 'activepieces',
      }),
    },
  ];

  public async validateHeaders(
    headers: Record<string, string>
  ): Promise<boolean> {
    try {
      const response = await axios({
        method: 'get',
        url:
          'https://www.googleapis.com/oauth2/v1/tokeninfo?access_token=' +
          headers['x-shinkai-oauth'] || headers[SHINKAI_OAUTH],
      });

      return response.status >= 200 && response.status < 300;
    } catch (e) {
      throw new Error(`Invalid "x-shinkai-oauth" header. 
        Please refresh the token or request a new one`);
    }
  }
}
