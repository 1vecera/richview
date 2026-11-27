import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {authorization,latestAnswer} from './windows/decision.mts';

export function selected(root: string,route: string,sha: string) {
  const record=JSON.parse(readFileSync(join(root,'selected-route.json'),'utf8')) as ReturnType<typeof authorization>;
  const expected=authorization(route,record.scope,sha,latestAnswer());
  if(JSON.stringify(expected)!==JSON.stringify(record))throw Error('Route, source or latest Q55 answer changed. Select the current route again.');
  return record;
}
