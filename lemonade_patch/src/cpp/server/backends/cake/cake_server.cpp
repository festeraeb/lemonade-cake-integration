#include "lemon/backends/cake/cake_server.h"

#include "lemon/backends/backend_ops.h"
#include "lemon/backends/backend_registry.h"
#include "lemon/backends/backend_utils.h"
#include "lemon/backends/cake/cake.h"
#include "lemon/utils/aixlog.hpp"
#include "lemon/utils/http_client.h"
#include "lemon/utils/path_utils.h"
#include "lemon/utils/process_manager.h"

#include <filesystem>
#include <stdexcept>
#include <string>
#include <vector>

namespace lemon { namespace backends {

CakeServer::CakeServer(const std::string& log_level,
                       ModelManager* model_manager,
                       BackendManager* backend_manager)
    : WrappedServer("lemonade-cake-bridge", log_level, model_manager, backend_manager) {}

CakeServer::~CakeServer() {
    unload();
}

// The bridge is not installed by Lemonade. Look for it, in order: LEMONADE_CAKE_BRIDGE (a full path),
// PATH, then the directory lemond runs from.
static std::string find_bridge_executable() {
    const std::string from_env = utils::get_environment_variable_utf8("LEMONADE_CAKE_BRIDGE");
    if (!from_env.empty()) {
        if (!std::filesystem::exists(from_env)) {
            throw std::runtime_error("LEMONADE_CAKE_BRIDGE points to a missing file: " + from_env);
        }
        return from_env;
    }
    const std::string binary = cake::spec()->binary;
    if (!utils::find_executable_in_path(binary).empty()) {
        return binary;
    }
    const std::filesystem::path beside = std::filesystem::path(utils::get_executable_dir()) / binary;
    if (std::filesystem::exists(beside)) {
        return beside.string();
    }
    throw std::runtime_error(binary + " not found: set LEMONADE_CAKE_BRIDGE, put it on PATH, or place it next to lemond");
}

void CakeServer::load(const std::string& model_name,
                      const ModelInfo& model_info,
                      const RecipeOptions& options,
                      bool /*do_not_upgrade*/) {
    const std::string checkpoint = model_info.checkpoint();
    if (checkpoint.empty()) {
        throw std::runtime_error("Cake model checkpoint is empty for: " + model_name);
    }

    // The recipe default is the number 18080 (cake.h); a request may also pass the port as a string.
    const json configured_port = options.get_option("cake_port");
    if (configured_port.is_number_integer()) {
        cake_private_port_ = configured_port.get<int>();
    } else if (configured_port.is_string() && !configured_port.get<std::string>().empty()) {
        cake_private_port_ = std::stoi(configured_port.get<std::string>());
    }

    port_ = choose_port();
    if (port_ == 0) {
        throw std::runtime_error("Failed to allocate Lemonade private port for Cake bridge");
    }

    const std::string bridge_executable = find_bridge_executable();

    std::vector<std::string> args = {
        "--listen-port", std::to_string(port_),
        "--cake-port", std::to_string(cake_private_port_),
        "--model", checkpoint,
        "--log-level", log_level_,
    };

    const std::string cake_args = options.get_option("cake_args");
    if (!cake_args.empty()) {
        args.push_back("--cake-command");
        args.push_back(cake_args);
    }

    LOG(INFO, "Cake") << "Starting Rust Cake bridge on port "
                       << port_ << " for model " << checkpoint << std::endl;

    const bool inherit_output = (log_level_ == "info") || is_debug();
    set_process_handle(utils::ProcessManager::start_process(
        bridge_executable,
        args,
        "",
        inherit_output,
        true,
        {}
    ));

    if (!wait_for_ready("/health")) {
        unload();
        throw std::runtime_error("lemonade-cake-bridge failed to become ready");
    }

    const json fingerprint = forward_get_request("/internal/runtime-fingerprint", 5);
    if (!fingerprint.value("runtime_identity_verified", false) ||
        fingerprint.value("observed_runtime", std::string{}) != "cake") {
        unload();
        throw std::runtime_error(
            "Cake runtime identity verification failed; refusing mislabeled backend");
    }

    start_backend_watchdog("/health");
    LOG(INFO, "Cake") << "Cake backend ready on bridge port " << port_ << std::endl;
}

void CakeServer::unload() {
    stop_backend_watchdog();

    const ProcessHandle handle = consume_process_handle_for_cleanup();
    if (has_process_handle(handle)) {
        utils::ProcessManager::stop_process(handle);
    }
}

json CakeServer::chat_completion(const json& request) {
    return forward_request("/v1/chat/completions", request);
}

json CakeServer::completion(const json& request) {
    return unsupported_capability_error(
        "legacy text completion; Cake integration currently exposes chat completion");
}

std::map<std::string, nlohmann::json> CakeServer::get_additional_telemetry() {
    if (get_backend_port() <= 0) {
        return {};
    }

    std::map<std::string, nlohmann::json> out;

    try {
        out["cake_topology"] = forward_get_request("/api/v1/topology", 2);
    } catch (...) {
        out["cake_topology"] = nullptr;
    }

    try {
        out["cake_capabilities"] = forward_get_request("/internal/capabilities", 2);
    } catch (...) {
        out["cake_capabilities"] = nullptr;
    }

    try {
        out["runtime_fingerprint"] =
            forward_get_request("/internal/runtime-fingerprint", 2);
    } catch (...) {
        out["runtime_fingerprint"] = nullptr;
    }

    return out;
}

std::string CakeServer::get_additional_telemetry_url() const {
    if (get_backend_port() <= 0) {
        return "";
    }
    return get_base_url() + "/api/v1/topology";
}

}}  // namespace lemon::backends

namespace lemon { namespace backends { namespace cake {

std::unique_ptr<WrappedServer> create(const BackendContext& ctx) {
    return make_server<CakeServer>(ctx);
}

const BackendSpec* spec() {
    return make_spec<CakeServer>(descriptor);
}

const BackendOps* ops() {
    return default_backend_ops();
}

}}}  // namespace lemon::backends::cake
