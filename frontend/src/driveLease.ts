/** Persist only this browser tab's own lease; never adopt another controller's session. */
export function driveLease(storage: Pick<Storage,'getItem'|'setItem'>, endpoint:string) {
  const key='microduck-drive:'+endpoint.replace(/\/$/,'');
  let session='',sequence=0;
  try {const saved=JSON.parse(storage.getItem(key)||'null');if(typeof saved?.session==='string' && Number.isSafeInteger(saved.sequence) && saved.sequence>=0){session=saved.session;sequence=saved.sequence;}} catch {}
  function save(){try{storage.setItem(key,JSON.stringify({session,sequence}));}catch{}}
  return {
    get session(){return session;},
    remember(value:string){if(value!==session){session=value;sequence=0;save();}},
    nextSequence(){sequence++;save();return sequence;},
  };
}
