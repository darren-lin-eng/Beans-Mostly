import sample from './sample.js';
import osm from './osm.js';
import kroger from './kroger.js';

export const providers = { sample, osm, kroger };
export const providerForStore = (storeId) => providers[String(storeId).split(':')[0]];
