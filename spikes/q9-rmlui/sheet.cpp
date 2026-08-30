// Hypothesis: decorators only paint when the document has a stylesheet context.
// Compare: (A) doc from LoadDocumentFromMemory with a <style> block, decorator
// set INLINE at runtime; (B) same doc, decorator applied via a CSS class.
#include <RmlUi/Core.h>
#include <cstdio>
#include <vector>
#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"

int main() {
    const int W=800,H=240;
    Backend::Initialize("sheet",W,H,false);
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();
    Rml::Context* ctx=Rml::CreateContext("m",Rml::Vector2i(W,H));

    const char* rml = R"(<rml><head><style>
      body { width: 800px; height: 240px; background-color: #202020; }
      div  { position: absolute; top: 40px; width: 160px; height: 160px; }
      .grad { decorator: linear-gradient(45deg, #11998e, #38ef7d); }
    </style></head><body>
      <div id="a" style="left:20px"/>
      <div id="b" style="left:200px"/>
      <div id="c" class="grad" style="left:380px"/>
    </body></rml>)";

    Rml::ElementDocument* doc = ctx->LoadDocumentFromMemory(rml);
    if(!doc){ printf("load failed\n"); return 1; }

    // A: control - background-color inline
    doc->GetElementById("a")->SetProperty("background-color","#e04040");
    // B: decorator set INLINE at runtime, on a doc that HAS a stylesheet
    bool ok = doc->GetElementById("b")->SetProperty("decorator","linear-gradient(45deg, #11998e, #38ef7d)");
    printf("  inline decorator set=%d\n",(int)ok);
    // C: decorator via stylesheet class (already applied)

    doc->Show();
    auto* gl3=static_cast<RenderInterface_GL3*>(Backend::GetRenderInterface());
    for(int i=0;i<3;i++){ ctx->Update(); Backend::BeginFrame(); ctx->Render(); gl3->EndFrame(); }

    std::vector<unsigned char> px((size_t)W*H*3);
    glReadPixels(0,0,W,H,GL_RGB,GL_UNSIGNED_BYTE,px.data());
    FILE* f=fopen("sheet.ppm","wb"); fprintf(f,"P6\n%d %d\n255\n",W,H);
    for(int y=H-1;y>=0;y--) fwrite(&px[(size_t)y*W*3],1,(size_t)W*3,f); fclose(f);
    auto s=[&](int x,int y){ size_t i=((size_t)(H-1-y)*W+x)*3;
        printf("%3d,%3d,%3d",px[i],px[i+1],px[i+2]); };
    printf("  A background-color (control) : "); s(100,120); printf("\n");
    printf("  B decorator set INLINE       : "); s(280,120); printf("\n");
    printf("  C decorator via STYLESHEET   : "); s(460,120); printf("\n");
    Rml::Shutdown(); Backend::Shutdown(); return 0;
}
