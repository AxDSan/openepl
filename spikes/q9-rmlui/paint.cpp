// Does a decorator actually PAINT when set as an inline property on a
// programmatically-created element (no stylesheet)? Four swatches, left to right.
#include <RmlUi/Core.h>
#include <cstdio>
#include <vector>
#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"

static Rml::Element* swatch(Rml::ElementDocument* doc, int x, const char* prop, const char* val) {
    Rml::Element* e = doc->AppendChild(doc->CreateElement("div"));
    e->SetProperty("position", "absolute");
    e->SetProperty("left", Rml::String(std::to_string(x) + "px"));
    e->SetProperty("top", "40px");
    e->SetProperty("width", "160px");
    e->SetProperty("height", "160px");
    bool ok = e->SetProperty(prop, val);
    printf("  swatch x=%3d  %-16s -> set=%d\n", x, prop, (int)ok);
    return e;
}

int main() {
    const int W = 800, H = 240;
    Backend::Initialize("paint", W, H, false);
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();
    Rml::Context* ctx = Rml::CreateContext("m", Rml::Vector2i(W, H));
    Rml::ElementDocument* doc = ctx->CreateDocument();
    doc->SetProperty("width", "800px"); doc->SetProperty("height", "240px");
    doc->SetProperty("background-color", "#202020");

    swatch(doc,  20, "background-color", "#e04040");                    // control
    swatch(doc, 200, "decorator", "linear-gradient(45deg, #11998e, #38ef7d)");
    swatch(doc, 380, "decorator", "conic-gradient(from 20deg, #ff5f6d, #24c6dc, #ff5f6d)");
    swatch(doc, 560, "decorator", "horizontal-gradient(#f00 #ff0)");    // legacy syntax

    doc->Show();
    auto* gl3 = static_cast<RenderInterface_GL3*>(Backend::GetRenderInterface());
    for (int i = 0; i < 3; i++) { ctx->Update(); Backend::BeginFrame(); ctx->Render(); gl3->EndFrame(); }

    std::vector<unsigned char> px((size_t)W*H*3);
    glReadPixels(0,0,W,H,GL_RGB,GL_UNSIGNED_BYTE,px.data());
    FILE* f=fopen("paint.ppm","wb"); fprintf(f,"P6\n%d %d\n255\n",W,H);
    for(int y=H-1;y>=0;y--) fwrite(&px[(size_t)y*W*3],1,(size_t)W*3,f);
    fclose(f);

    // Sample the centre of each swatch.
    auto sample=[&](int x,int y){ size_t i=((size_t)(H-1-y)*W+x)*3; return Rml::String(
        std::to_string(px[i])+","+std::to_string(px[i+1])+","+std::to_string(px[i+2])); };
    printf("\n  centre pixels (expect non-#202020 for each):\n");
    printf("    background-color swatch : %s\n", sample(100,120).c_str());
    printf("    linear-gradient  swatch : %s\n", sample(280,120).c_str());
    printf("    conic-gradient   swatch : %s\n", sample(460,120).c_str());
    printf("    horizontal-grad  swatch : %s\n", sample(640,120).c_str());
    Rml::Shutdown(); Backend::Shutdown(); return 0;
}
