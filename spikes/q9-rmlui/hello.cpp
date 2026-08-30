// Minimal RmlUi app, for a like-for-like size comparison with the Rust toolkits.
#include <RmlUi/Core.h>
#include "RmlUi_Backend.h"
int main() {
    if (!Backend::Initialize("hello", 800, 600, true)) return 1;
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();
    Rml::Context* ctx = Rml::CreateContext("m", Rml::Vector2i(800, 600));
    Rml::LoadFontFace("/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf");
    Rml::ElementDocument* doc = ctx->LoadDocumentFromMemory(
        "<rml><head><style>body{font-family:'Adwaita Mono';color:#fff}</style></head>"
        "<body>Hello</body></rml>");
    if (doc) doc->Show();
    while (Backend::ProcessEvents(ctx)) { ctx->Update(); Backend::BeginFrame(); ctx->Render(); Backend::PresentFrame(); }
    Rml::Shutdown(); Backend::Shutdown(); return 0;
}
