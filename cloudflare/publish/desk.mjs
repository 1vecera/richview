// Host reason (ARC-03): the embedded source editor on the signed-in desk page. It mounts a text editor and
// calls the desk's JSON API; validation, rendering and every rule run on the server in Rust. It shows the
// two browser actions the lockdown allows (INT-26, FEA-BR-27): approve or reject a pending change, and an
// author's quick source edit with diagnostics at their source lines. The editor library is open (ARC-03);
// until one is chosen this is a plain text area with line numbers.
export const deskHTML = `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>RichView Publish desk</title>
<style>body{font:15px/1.5 system-ui,sans-serif;margin:0 auto;max-width:960px;padding:24px}h1{font-size:20px}section{margin:24px 0}li{margin:6px 0}button{font:inherit;padding:4px 12px;margin-right:6px}
.src{display:grid;grid-template-columns:auto 1fr;border:1px solid #888;font:13px/1.5 ui-monospace,monospace}.nums{padding:8px;text-align:right;color:#777;background:#f3f3f3;white-space:pre;user-select:none}textarea{border:0;padding:8px;min-height:420px;font:inherit;white-space:pre;overflow:auto;resize:vertical}
.err{color:#a00}.ok{color:#060}</style>
<main><h1>RichView Publish desk</h1><p id="who"></p><section><h2>Changes awaiting review</h2><ul id="changes"></ul></section><section><h2>Your items</h2><ul id="items"></ul></section>
<section id="edit" hidden><h2 id="title"></h2><div class="src"><div class="nums" id="nums"></div><textarea id="text" spellcheck="false"></textarea></div><p><button id="save">Save as a pull request</button><span id="msg"></span></p><ul id="diag"></ul></section></main>
<script src="/_editor/app.js"></script></html>`;

export const deskScript = `
const $=id=>document.getElementById(id);
const api=async(path,body)=>{const r=await fetch("/_editor/api/"+path,body?{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(body)}:{});
const type=r.headers.get("content-type")||"";const data=type.includes("json")?await r.json():{error:r.statusText};return{ok:r.ok,status:r.status,data};};
let current=null;
const li=(text,...buttons)=>{const e=document.createElement("li");e.append(text+" ");buttons.forEach(([label,fn])=>{const b=document.createElement("button");b.textContent=label;b.onclick=fn;e.append(b);});return e;};
const lines=()=>{$("nums").textContent=$("text").value.split("\\n").map((_,i)=>i+1).join("\\n");};
async function load(){
 const session=await api("session");$("who").textContent=session.ok?session.data.actor+" · "+session.data.role.name:"Not signed in";
 const list=await api("items");if(!list.ok)return;
 $("changes").replaceChildren(...list.data.changes.filter(c=>c.state==="pending review"||c.state==="approved").map(c=>li("#"+c.number+" "+c.state+" "+c.head.slice(0,8)+" ",
  ["Approve",async()=>{const r=await api("approve",{number:c.number,head:c.head});alert(r.ok?"Approved":r.data.error||r.status);load();}],
  ["Reject",async()=>{const reason=prompt("Reason")||"";const r=await api("reject",{number:c.number,head:c.head,reason});alert(r.ok?"Closed":r.data.error||r.status);load();}])));
 $("items").replaceChildren(...list.data.items.map(i=>li(i.title+" ("+i.state+(i.private?", private":"")+") ",["Edit source",()=>open(i)],...(i.private?[["Make public",async()=>{const r=await api("approve",{id:i.id});alert(r.ok?"Public":r.data.error||r.status);load();}]]:[]))));
}
async function open(item){
 const r=await api("source?id="+encodeURIComponent(item.id));if(!r.ok){alert("Unavailable: "+(r.data.error||r.status));return;}
 current={item,path:r.data.path,revision:r.data.revision};$("title").textContent=item.title;$("text").value=r.data.source;$("edit").hidden=false;$("diag").replaceChildren();$("msg").textContent="";lines();
}
$("text").oninput=lines;
$("save").onclick=async()=>{
 $("msg").textContent="Validating…";$("diag").replaceChildren();
 const r=await api("edit",{path:current.path,revision:current.revision,source:$("text").value});
 if(r.ok){$("msg").className="ok";$("msg").textContent="Saved as pull request #"+r.data.number;load();return;}
 $("msg").className="err";$("msg").textContent=r.data.error||("Refused ("+r.status+"). Your text is kept.");
 for(const d of r.data.diagnostics||[])$("diag").append(li((d.line?"line "+d.line+": ":"")+d.message+" ["+d.code+"]"));
};
load();
`;
