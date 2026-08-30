// Focused diagnostic: which property sets actually succeed, and do decorators parse?
#include <RmlUi/Core.h>
#include <cstdio>
#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"

static void trySet(Rml::Element* e, const char* k, const char* v) {
    bool ok = e->SetProperty(k, v);
    const Rml::Property* p = ok ? e->GetProperty(k) : nullptr;
    printf("  %-14s = %-62s %s%s\n", k, v, ok ? "ok" : "*** REJECTED ***",
           (ok && p) ? "" : "");
}

int main() {
    Backend::Initialize("diag", 640, 400, false);
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();
    Rml::Context* ctx = Rml::CreateContext("m", Rml::Vector2i(640, 400));
    Rml::ElementDocument* doc = ctx->CreateDocument();

    printf("\n-- gradient decorator syntaxes --\n");
    Rml::Element* a = doc->AppendChild(doc->CreateElement("div"));
    trySet(a, "decorator", "conic-gradient(from 20deg at 30% 40%, #ff5f6d, #ffc371, #24c6dc)");
    trySet(a, "decorator", "linear-gradient(45deg, #11998e, #38ef7d)");
    trySet(a, "decorator", "horizontal-gradient(#f00 #ff0)");
    trySet(a, "decorator", "radial-gradient(#f00, #00f)");

    printf("\n-- filters / transforms --\n");
    trySet(a, "filter", "drop-shadow(#000a 6px 8px 10px)");
    trySet(a, "filter", "blur(10px)");
    trySet(a, "backdrop-filter", "blur(22px)");
    trySet(a, "transform", "rotate(-8deg) scale(1.05)");
    trySet(a, "mask-image", "linear-gradient(#fff, #0000)");
    trySet(a, "box-shadow", "#0008 0px 18px 40px 4px");
    trySet(a, "font-effect", "glow(3px #fff8)");

    printf("\n-- animation/transition --\n");
    trySet(a, "transition", "background-color 0.3s ease-in-out");
    trySet(a, "animation", "2s cubic-in-out infinite alternate pulse");

    printf("\n-- does an absolutely-positioned sibling get laid out? --\n");
    Rml::Element* b = doc->AppendChild(doc->CreateElement("div"));
    b->SetProperty("position", "absolute");
    b->SetProperty("left", "100px"); b->SetProperty("top", "50px");
    b->SetProperty("width", "200px"); b->SetProperty("height", "120px");
    b->SetProperty("background-color", "#ff0000");
    doc->Show();
    ctx->Update();
    auto box = b->GetBox();
    printf("  sibling box size = %g x %g, offset = (%g, %g), visible=%d\n",
           box.GetSize().x, box.GetSize().y,
           b->GetAbsoluteOffset().x, b->GetAbsoluteOffset().y, (int)b->IsVisible());
    auto abox = a->GetBox();
    printf("  first div box size = %g x %g (no width/height set)\n", abox.GetSize().x, abox.GetSize().y);

    Rml::Shutdown(); Backend::Shutdown();
    return 0;
}
