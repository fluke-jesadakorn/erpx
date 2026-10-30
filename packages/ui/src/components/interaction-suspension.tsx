'use client';
import {createContext} from 'react';

/** Keep mounted work intact while its owning identity is unavailable.
 * Portaled controls inherit this context even outside the guarded DOM tree. */
export const InteractionSuspension = createContext(false);
