export const GET_PUBLIC_KEY = 'GET_PUBLIC_KEY';
export const USE_REGISTRATION_CODE = 'USE_REGISTRATION_CODE';
export const CREATE_REGISTRATION_CODE = 'CREATE_REGISTRATION_CODE';
export const REGISTRATION_ERROR = 'REGISTRATION_ERROR';
export const PING_ALL = 'PING_ALL';
export const CLEAR_REGISTRATION_CODE = 'CLEAR_REGISTRATION_CODE';
export const RECEIVE_LAST_MESSAGES_FROM_INBOX = "RECEIVE_LAST_MESSAGES_FROM_INBOX";
export const RECEIVE_LOAD_MORE_MESSAGES_FROM_INBOX = "RECEIVE_LOAD_MORE_MESSAGES_FROM_INBOX";
export const CLEAR_STORE = 'CLEAR_STORE';
export const ADD_MESSAGE_TO_INBOX = 'ADD_MESSAGE_TO_INBOX';
export const RECEIVE_ALL_INBOXES_FOR_PROFILE = 'RECEIVE_ALL_INBOXES_FOR_PROFILE';

export interface Action {
    type: string;
    payload?: any;
  }