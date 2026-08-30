/* Core commands: text (slot ABI). Results allocated through the channel. */
#include <ctype.h>
#include <string.h>
#include "openepl_core.h"

static const char *nz(const char *s){ return s?s:""; }
static char *astr(long len){ return (char*)oe_malloc(len+1); }

void oe_length(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_int(r,(int)strlen(nz(oe_arg_text(argv,0)))); }

void oe_uppercase(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*s=nz(oe_arg_text(argv,0)); long n=(long)strlen(s); char*o=astr(n);
    for(long i=0;i<n;i++) o[i]=(char)toupper((unsigned char)s[i]); o[n]='\0'; oe_ret_text(r,o);
}
void oe_lowercase(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*s=nz(oe_arg_text(argv,0)); long n=(long)strlen(s); char*o=astr(n);
    for(long i=0;i<n;i++) o[i]=(char)tolower((unsigned char)s[i]); o[n]='\0'; oe_ret_text(r,o);
}
void oe_trim(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*s=nz(oe_arg_text(argv,0));
    const char*a=s; while(*a && isspace((unsigned char)*a)) a++;
    const char*e=s+strlen(s); while(e>a && isspace((unsigned char)e[-1])) e--;
    long n=e-a; char*o=astr(n); memcpy(o,a,n); o[n]='\0'; oe_ret_text(r,o);
}
void oe_substr(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*s=nz(oe_arg_text(argv,0)); int start=oe_arg_int(argv,1), count=oe_arg_int(argv,2);
    long len=(long)strlen(s);
    if(start<0)start=0; if(start>len)start=(int)len; if(count<0)count=0;
    long avail=len-start, n=count<avail?count:avail;
    char*o=astr(n); memcpy(o,s+start,n); o[n]='\0'; oe_ret_text(r,o);
}
void oe_find(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*h=nz(oe_arg_text(argv,0)), *n=nz(oe_arg_text(argv,1));
    const char*hit=strstr(h,n); oe_ret_int(r, hit?(int)(hit-h):-1);
}
void oe_concat(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*a=nz(oe_arg_text(argv,0)),*b=nz(oe_arg_text(argv,1));
    long la=(long)strlen(a),lb=(long)strlen(b); char*o=astr(la+lb);
    memcpy(o,a,la); memcpy(o+la,b,lb); o[la+lb]='\0'; oe_ret_text(r,o);
}
void oe_repeat(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*s=nz(oe_arg_text(argv,0)); int times=oe_arg_int(argv,1); if(times<0)times=0;
    long n=(long)strlen(s), total=n*(long)times; char*o=astr(total); char*p=o;
    for(int i=0;i<times;i++){ memcpy(p,s,n); p+=n; } *p='\0'; oe_ret_text(r,o);
}
void oe_reverse(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; const char*s=nz(oe_arg_text(argv,0)); long n=(long)strlen(s); char*o=astr(n);
    for(long i=0;i<n;i++) o[i]=s[n-1-i]; o[n]='\0'; oe_ret_text(r,o);
}
void oe_replace(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c;
    const char*s=nz(oe_arg_text(argv,0)),*from=nz(oe_arg_text(argv,1)),*to=nz(oe_arg_text(argv,2));
    long flen=(long)strlen(from);
    if(flen==0){ long n=(long)strlen(s); char*o=astr(n); memcpy(o,s,n+1); oe_ret_text(r,o); return; }
    long tlen=(long)strlen(to), count=0;
    for(const char*p=s;(p=strstr(p,from));p+=flen) count++;
    long slen=(long)strlen(s), outlen=slen+count*(tlen-flen);
    char*o=astr(outlen); char*w=o; const char*p=s;
    for(;;){ const char*hit=strstr(p,from);
        if(!hit){ long rest=(long)strlen(p); memcpy(w,p,rest); w+=rest; break; }
        long chunk=hit-p; memcpy(w,p,chunk); w+=chunk; memcpy(w,to,tlen); w+=tlen; p=hit+flen; }
    *w='\0'; oe_ret_text(r,o);
}
