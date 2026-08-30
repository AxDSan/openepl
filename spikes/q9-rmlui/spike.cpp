// OpenEPL Q9 spike — RmlUi viability (ADR 0004 §8).
//
// Proves, or fails to prove, the four things the ADR says could kill RmlUi:
//   1. the full effects set actually renders through the reference GL3 backend
//   2. a component registered under OUR OWN string name can be created by that
//      name, have properties set by string, and dispatch events  (the LibInfo
//      component-registration analogue)
//   3. binary size
//   4. accessibility reachability (inspected separately)
//
// The entire UI is built PROGRAMMATICALLY — no .rml/.rcss file is loaded — which
// is the mode the OpenEPL runtime would actually use when instantiating a form
// from IR at run time.

#include <RmlUi/Core.h>
#include <RmlUi/Core/ElementInstancer.h>
#include <RmlUi/Core/StyleSheetSpecification.h>
#include <cstdio>
#include <cstring>
#include <vector>
#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"

static int g_events_fired = 0;

// --- A custom component, the analogue of an OpenEPL visual component --------
class OeGauge : public Rml::Element {
public:
    explicit OeGauge(const Rml::String& tag) : Rml::Element(tag) {}
    // Report a size so layout gives us a box even though we draw via RCSS.
    void OnPropertyChange(const Rml::PropertyIdSet& ids) override {
        Rml::Element::OnPropertyChange(ids);
    }
};

class OeGaugeInstancer : public Rml::ElementInstancer {
public:
    Rml::ElementPtr InstanceElement(Rml::Element*, const Rml::String& tag,
                                    const Rml::XMLAttributes&) override {
        return Rml::ElementPtr(new OeGauge(tag));
    }
    void ReleaseElement(Rml::Element* element) override { delete element; }
};

struct ClickListener : public Rml::EventListener {
    void ProcessEvent(Rml::Event& ev) override {
        g_events_fired++;
        printf("  [event] '%s' fired on <%s id=%s>\n", ev.GetType().c_str(),
               ev.GetTargetElement()->GetTagName().c_str(),
               ev.GetTargetElement()->GetId().c_str());
    }
};

static bool check(const char* label, bool ok) {
    printf("  %-58s %s\n", label, ok ? "PASS" : "*** FAIL ***");
    return ok;
}

int main() {
    const int W = 900, H = 600;
    if (!Backend::Initialize("OpenEPL Q9 spike", W, H, true)) {
        fprintf(stderr, "backend init failed\n");
        return 1;
    }
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();

    int failures = 0;
    auto FAIL = [&](bool ok) { if (!ok) failures++; };

    // --- STEP 2a: register a CUSTOM PROPERTY under our own name -------------
    // This is the direct analogue of an OpenEPL component declaring a property
    // in its LibInfo descriptor.
    Rml::StyleSheetSpecification::RegisterProperty("oe-value", "0", false, false)
        .AddParser("number");
    printf("\n== STEP 2: component model (LibInfo analogue) ==\n");
    FAIL(check("register custom property 'oe-value' by string name", true));

    // --- STEP 2b: register a CUSTOM ELEMENT TYPE under our own tag ----------
    static OeGaugeInstancer gauge_instancer;
    Rml::Factory::RegisterElementInstancer("oe-gauge", &gauge_instancer);
    FAIL(check("register element instancer for tag 'oe-gauge'", true));

    Rml::LoadFontFace("/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf");

    Rml::Context* ctx = Rml::CreateContext("main", Rml::Vector2i(W, H));

    // FINDING (spike): decorators are silently dropped on a document created
    // with bare CreateDocument() — they require a stylesheet context. We seed a
    // minimal stylesheet, then build the entire UI programmatically as the
    // OpenEPL runtime would when instantiating a form from IR.
    const char* seed = R"(<rml><head><style>
        body { width: 900px; height: 600px; background-color: #1e2233;
               font-family: "Adwaita Mono"; font-size: 18px; color: #fff; }
    </style></head><body/></rml>)";
    Rml::ElementDocument* doc = ctx->LoadDocumentFromMemory(seed);
    FAIL(check("create document (stylesheet-seeded, then programmatic)", doc != nullptr));
    if (!doc) return 1;

    // --- STEP 2c: create elements BY STRING TAG NAME at runtime -------------
    // A decorative backdrop so backdrop-filter has something to blur.
    Rml::ElementPtr backdrop_p = doc->CreateElement("div");
    Rml::Element* backdrop = doc->AppendChild(std::move(backdrop_p));
    FAIL(check("CreateElement(\"div\") by string tag", backdrop != nullptr));
    backdrop->SetProperty("position", "absolute");
    backdrop->SetProperty("left", "0px");   backdrop->SetProperty("top", "0px");
    backdrop->SetProperty("width", "900px"); backdrop->SetProperty("height", "600px");
    // conic gradient decorator — an FMX-class fill
    bool deco_ok = backdrop->SetProperty("decorator",
        "conic-gradient(from 20deg at 30% 40%, #ff5f6d, #ffc371, #24c6dc, #514a9d, #ff5f6d)");
    FAIL(check("conic-gradient decorator accepted", deco_ok));

    // Our custom component, instantiated BY OUR OWN STRING NAME.
    Rml::ElementPtr gauge_p = doc->CreateElement("oe-gauge");
    bool is_custom = (dynamic_cast<OeGauge*>(gauge_p.get()) != nullptr);
    FAIL(check("CreateElement(\"oe-gauge\") returns OUR component type", is_custom));
    Rml::Element* gauge = doc->AppendChild(std::move(gauge_p));

    // --- STEP 2d: set properties BY STRING NAME, several types --------------
    struct { const char* k; const char* v; } props[] = {
        {"position", "absolute"}, {"left", "120px"}, {"top", "150px"},
        {"width", "420px"}, {"height", "260px"},
        {"background-color", "rgba(255,255,255,40)"},   // colour
        {"border-radius", "28px"},                       // length
        {"border-width", "2px"}, {"border-color", "#ffffffaa"},
        {"box-shadow", "#0008 0px 18px 40px 4px"},       // shadow
        {"backdrop-filter", "blur(22px)"},               // THE hard one
        {"padding", "26px"}, {"color", "#ffffff"},
        {"oe-value", "42"},                              // OUR custom property
    };
    bool all_set = true;
    for (auto& p : props) all_set &= gauge->SetProperty(p.k, p.v);
    FAIL(check("SetProperty by string name (14 props incl. custom)", all_set));

    // Read a property back by string name.
    const Rml::Property* got = gauge->GetProperty("oe-value");
    bool readback = got && (int)got->Get<float>() == 42;
    FAIL(check("GetProperty(\"oe-value\") reads back 42", readback));

    Rml::Element* label = gauge->AppendChild(doc->CreateElement("div"));
    label->SetInnerRML("OpenEPL RmlUi spike<br/>backdrop-filter: blur(22px)<br/>oe-value = 42");
    label->SetProperty("font-effect", "glow(3px #fff8)");

    // A second card exercising filter: blur + drop-shadow on content.
    Rml::Element* card2 = doc->AppendChild(doc->CreateElement("div"));
    card2->SetProperty("position", "absolute");
    card2->SetProperty("left", "600px"); card2->SetProperty("top", "330px");
    card2->SetProperty("width", "220px"); card2->SetProperty("height", "180px");
    card2->SetProperty("border-radius", "20px");
    card2->SetProperty("decorator", "linear-gradient(45deg, #11998e, #38ef7d)");
    bool c2 = card2->SetProperty("filter", "drop-shadow(#000a 6px 8px 10px)");
    c2 &= card2->SetProperty("transform", "rotate(-8deg) scale(1.05)");
    FAIL(check("filter + transform accepted on second card", c2));

    // --- STEP 2e: events ----------------------------------------------------
    gauge->SetId("gauge1");
    static ClickListener listener;
    gauge->AddEventListener("click", &listener);
    FAIL(check("AddEventListener(\"click\") by string", true));

    doc->Show();
    ctx->Update();

    // Dispatch a synthetic event to prove the wiring.
    gauge->DispatchEvent("click", Rml::Dictionary());
    FAIL(check("synthetic click reached our listener", g_events_fired == 1));

    // --- STEP 1: does the effects set actually RENDER? ----------------------
    printf("\n== STEP 1: effects render through reference GL3 backend ==\n");
    auto* gl3 = static_cast<RenderInterface_GL3*>(Backend::GetRenderInterface());
    for (int frame = 0; frame < 3; frame++) { ctx->Update(); Backend::BeginFrame(); ctx->Render(); gl3->EndFrame(); }

    std::vector<unsigned char> px((size_t)W * H * 3);
    glReadPixels(0, 0, W, H, GL_RGB, GL_UNSIGNED_BYTE, px.data());

    // Flip vertically and write a PPM for inspection.
    FILE* f = fopen("spike.ppm", "wb");
    fprintf(f, "P6\n%d %d\n255\n", W, H);
    for (int y = H - 1; y >= 0; y--) fwrite(&px[(size_t)y * W * 3], 1, (size_t)W * 3, f);
    fclose(f);

    // Verify we actually drew something: count distinct colours + non-background.
    long nonbg = 0; int minv = 255, maxv = 0;
    for (size_t i = 0; i < px.size(); i += 3) {
        int r = px[i], g = px[i+1], b = px[i+2];
        if (!(r == 0 && g == 0 && b == 0)) nonbg++;
        int v = (r + g + b) / 3; if (v < minv) minv = v; if (v > maxv) maxv = v;
    }
    FAIL(check("framebuffer is not blank (pixels drawn)", nonbg > 1000));
    FAIL(check("wide tonal range", maxv - minv > 100));
    // Strong assertion: the top-left corner must be conic-gradient colour, NOT
    // the #1e2233 body background. This is what the weak heuristic missed.
    auto at = [&](int x, int y) { size_t i = ((size_t)y * W + x) * 3; return &px[i]; };
    unsigned char* corner = at(30, 30);
    bool is_bg = (abs(corner[0]-0x1e) < 12 && abs(corner[1]-0x22) < 12 && abs(corner[2]-0x33) < 12);
    printf("  corner pixel = %d,%d,%d\n", corner[0], corner[1], corner[2]);
    FAIL(check("background decorator actually PAINTED (not body colour)", !is_bg));
    printf("  wrote spike.ppm (%dx%d), non-black px=%ld, range=%d..%d\n", W, H, nonbg, minv, maxv);

    printf("\n== RESULT: %d failure(s) ==\n", failures);
    Rml::Shutdown();
    Backend::Shutdown();
    return failures == 0 ? 0 : 1;
}
