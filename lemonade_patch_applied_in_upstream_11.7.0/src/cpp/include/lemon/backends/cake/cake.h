#pragma once

#include "lemon/backends/backend_descriptor.h"

namespace lemon { namespace backends { namespace cake {

// The cake backend descriptor (plain data). Header-only `inline const` so it
// links into both the lemonade CLI and lemond without a separate source file.
inline const BackendDescriptor descriptor = {
    /*recipe*/              "cake",
    /*display_name*/        "Cake (Rust heterogeneous runtime)",
    /*binary*/              "lemonade-cake-bridge",
    /*config_section*/      "cake",
    /*default_device*/      DEVICE_GPU,
    /*slot_policy*/         SlotPolicy::Standard,
    /*selectable_backend*/  false,
    /*uses_ctx_size*/       false,
    /*dynamic_models*/      false,
    /*options*/ {
        {"cake_args", "--cake-args", "", "ARGS",
         "Cake command template or extra bridge arguments", "Cake Options"},
        {"cake_port", "--cake-port", 18080, "PORT",
         "Private Cake port used behind the Rust bridge", "Cake Options"},
    },
    /*support*/ {},
    /*supported_modes*/ {"chat"},
    /*required_checkpoints*/ {"main"},
    /*default_capabilities*/ {},
    /*experimental*/        true,
    /*web_display_name*/    "",
    /*rocm_channels*/       {},
    /*exposes_prometheus_metrics*/ false,
    /*rocm_requires_cwsr_fix*/ false,
    /*version_policy*/      VersionPolicy::Exact,
    /*self_manages_downloads*/ true,   // `cake run/pull` fetches models from HF itself
    /*takes_args*/          false,
    /*arg_variants*/        {},
    /*bin_variants*/        {},
    /*config_extra*/        nlohmann::json::object(),
};

}}}  // namespace lemon::backends::cake

