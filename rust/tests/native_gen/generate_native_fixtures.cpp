/*
 * Native CSXCAD fixture generator for the Rust migration corpus.
 *
 * This program links against the *native* CSXCAD (and openEMS) shared
 * libraries and emits:
 *
 *   <out>/xml/<name>.xml        XML written by ContinuousStructure::Write2XML
 *   <out>/native/<name>.report  model observables read back from the native
 *                               C++ API (property/primitive inventory, bounds,
 *                               containment queries, grid, parameters,
 *                               transforms, expression error codes)
 *   <out>/xml/manifest.txt      fixture inventory
 *
 * The generated files are committed under CSXCAD/rust/tests/fixtures and are
 * the reference side of RM-028 (XML round trip), RM-029 (differential model
 * harness) and RM-034 (transform/parameter parity). They must never be edited
 * by hand: re-run tests/native_gen/run_native_generator.sh.
 *
 * Build/run instructions: tests/native_gen/README.md
 */

#include <cmath>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "ContinuousStructure.h"
#include "CSPrimBox.h"
#include "CSPrimCurve.h"
#include "CSPrimCylinder.h"
#include "CSPrimCylindricalShell.h"
#include "CSPrimLinPoly.h"
#include "CSPrimMultiBox.h"
#include "CSPrimPoint.h"
#include "CSPrimPolygon.h"
#include "CSPrimPolyhedron.h"
#include "CSPrimPolyhedronReader.h"
#include "CSPrimRotPoly.h"
#include "CSPrimSphere.h"
#include "CSPrimSphericalShell.h"
#include "CSPrimUserDefined.h"
#include "CSPrimWire.h"
#include "CSPropAbsorbingBC.h"
#include "CSPropConductingSheet.h"
#include "CSPropDebyeMaterial.h"
#include "CSPropDiscMaterial.h"
#include "CSPropDispersiveMaterial.h"
#include "CSPropDumpBox.h"
#include "CSPropExcitation.h"
#include "CSPropLorentzMaterial.h"
#include "CSPropLumpedElement.h"
#include "CSPropMaterial.h"
#include "CSPropMetal.h"
#include "CSPropProbeBox.h"
#include "CSPropResBox.h"
#include "CSPropUnknown.h"
#include "CSTransform.h"

#include "openems.h"

namespace {

std::string out_dir;

std::vector<std::pair<size_t, size_t>> manifest_counts;

std::string num(double value) {
    if (std::isnan(value)) return "nan";
    if (std::isinf(value)) return value > 0 ? "inf" : "-inf";
    char buffer[64];
    std::snprintf(buffer, sizeof(buffer), "%.17g", value);
    return buffer;
}

struct Query {
    double x, y, z;
};

struct Report {
    std::ostringstream body;
    std::vector<std::string> preamble;
    std::string fixture;

    void line(const std::string& text) { body << text << '\n'; }

    void property(ContinuousStructure& cs, size_t index) {
        CSProperties* prop = cs.GetProperty(index);
        RGBa fill = prop->GetFillColor();
        RGBa edge = prop->GetEdgeColor();
        std::ostringstream out;
        out << "property|" << index << '|' << prop->GetTypeXMLString() << '|'
            << prop->GetName() << '|' << (prop->GetVisibility() ? 1 : 0) << '|'
            << int(fill.R) << '|' << int(fill.G) << '|' << int(fill.B) << '|'
            << int(fill.a) << '|' << int(edge.R) << '|' << int(edge.G) << '|'
            << int(edge.B) << '|' << int(edge.a) << '|'
            << prop->GetCoordInputType() << '|' << prop->GetQtyPrimitives();
        line(out.str());
        for (const std::string& name : prop->GetAttributeNames()) {
            line("attr|" + std::to_string(index) + "|" + name + "|" +
                 prop->GetAttributeValue(name));
        }
        reportPropertyDetails(prop, index);
    }

    void reportPropertyDetails(CSProperties* prop, size_t index) {
        const std::string prefix = "detail|" + std::to_string(index) + "|";
        if (CSPropMaterial* mat = prop->ToMaterial()) {
            line(prefix + "isotropy|" + (mat->GetIsotropy() ? "1" : "0"));
            for (int ny = 0; ny < 3; ++ny) {
                const std::string suffix = "|" + std::to_string(ny);
                line(prefix + "epsilon" + suffix + "|" + num(mat->GetEpsilon(ny)));
                line(prefix + "mue" + suffix + "|" + num(mat->GetMue(ny)));
                line(prefix + "kappa" + suffix + "|" + num(mat->GetKappa(ny)));
                line(prefix + "sigma" + suffix + "|" + num(mat->GetSigma(ny)));
            }
            line(prefix + "density|" + num(mat->GetDensity()));
            for (int ny = 0; ny < 3; ++ny) {
                const std::string suffix = "|" + std::to_string(ny);
                line(prefix + "weight_epsilon" + suffix + "|" +
                     mat->GetEpsilonWeightFunction(ny));
                line(prefix + "weight_kappa" + suffix + "|" +
                     mat->GetKappaWeightFunction(ny));
            }
            line(prefix + "weight_density|" + mat->GetDensityWeightFunction());
        }
        if (CSPropDebyeMaterial* debye = prop->ToDebyeMaterial()) {
            line(prefix + "dispersion_order|" +
                 std::to_string(debye->GetDispersionOrder()));
            for (int order = 0; order < debye->GetDispersionOrder(); ++order) {
                for (int ny = 0; ny < 3; ++ny) {
                    const std::string suffix =
                        "|" + std::to_string(order) + "|" + std::to_string(ny);
                    line(prefix + "debye_delta" + suffix + "|" +
                         num(debye->GetEpsDelta(order, ny)));
                    line(prefix + "debye_relax" + suffix + "|" +
                         num(debye->GetEpsRelaxTime(order, ny)));
                }
                line(prefix + "debye_delta_weight|" + std::to_string(order) + "|" +
                     debye->GetEpsDeltaWeightFunction(order, 0));
            }
        }
        if (CSPropLorentzMaterial* lorentz = prop->ToLorentzMaterial()) {
            line(prefix + "dispersion_order|" +
                 std::to_string(lorentz->GetDispersionOrder()));
            for (int order = 0; order < lorentz->GetDispersionOrder(); ++order) {
                const std::string o = std::to_string(order);
                line(prefix + "lorentz_eps_plasma|" + o + "|" +
                     num(lorentz->GetEpsPlasmaFreq(order, 0)));
                line(prefix + "lorentz_mue_plasma|" + o + "|" +
                     num(lorentz->GetMuePlasmaFreq(order, 0)));
                line(prefix + "lorentz_eps_pole|" + o + "|" +
                     num(lorentz->GetEpsLorPoleFreq(order, 0)));
                line(prefix + "lorentz_mue_pole|" + o + "|" +
                     num(lorentz->GetMueLorPoleFreq(order, 0)));
                line(prefix + "lorentz_eps_relax|" + o + "|" +
                     num(lorentz->GetEpsRelaxTime(order, 0)));
                line(prefix + "lorentz_mue_relax|" + o + "|" +
                     num(lorentz->GetMueRelaxTime(order, 0)));
            }
        }
        if (CSPropDiscMaterial* disc = prop->ToDiscMaterial()) {
            line(prefix + "disc_file|" + disc->GetFilename());
            line(prefix + "disc_scale|" + num(disc->GetScale()));
            line(prefix + "disc_use_db_background|" +
                 (disc->GetUseDataBaseForBackground() ? "1" : "0"));
        }
        if (CSPropConductingSheet* sheet = prop->ToConductingSheet()) {
            line(prefix + "conductivity|" + num(sheet->GetConductivity()));
            line(prefix + "thickness|" + num(sheet->GetThickness()));
        }
        if (auto* lumped = dynamic_cast<CSPropLumpedElement*>(prop)) {
            line(prefix + "lumped_direction|" +
                 std::to_string(lumped->GetDirection()));
            line(prefix + "lumped_caps|" +
                 std::to_string(lumped->GetCaps()));
            line(prefix + "lumped_R|" + num(lumped->GetResistance()));
            line(prefix + "lumped_C|" + num(lumped->GetCapacity()));
            line(prefix + "lumped_L|" + num(lumped->GetInductance()));
            line(prefix + "lumped_type|" +
                 std::to_string(int(lumped->GetLEtype())));
        }
        if (auto* abc = dynamic_cast<CSPropAbsorbingBC*>(prop)) {
            line(prefix + "abc_normal_sign|" +
                 (abc->GetNormalSignPositive() ? "1" : "0"));
            line(prefix + "abc_phase_velocity|" + num(abc->GetPhaseVelocity()));
            line(prefix + "abc_type|" +
                 std::to_string(int(abc->GetAbsorbingBoundaryType())));
        }
        if (CSPropExcitation* excitation = prop->ToExcitation()) {
            line(prefix + "excitation_number|" +
                 std::to_string(excitation->GetNumber()));
            line(prefix + "excitation_enabled|" +
                 (excitation->GetEnabled() ? "1" : "0"));
            line(prefix + "excitation_type|" +
                 std::to_string(excitation->GetExcitType()));
            line(prefix + "excitation_frequency|" +
                 num(excitation->GetFrequency()));
            line(prefix + "excitation_delay|" + num(excitation->GetDelay()));
            for (int ny = 0; ny < 3; ++ny) {
                const std::string suffix = "|" + std::to_string(ny);
                line(prefix + "excitation_component" + suffix + "|" +
                     num(excitation->GetExcitation(ny)));
                line(prefix + "excitation_propdir" + suffix + "|" +
                     num(excitation->GetPropagationDir(ny)));
                line(prefix + "excitation_weight" + suffix + "|" +
                     excitation->GetWeightFunction(ny));
                line(prefix + "excitation_origin" + suffix + "|" +
                     num(excitation->GetWeightOrigin(ny)));
            }
            line(prefix + "excitation_weight_file|" +
                 excitation->GetWeightFile());
        }
        if (CSPropProbeBox* probe = prop->ToProbeBox()) {
            line(prefix + "probe_number|" +
                 std::to_string(probe->GetNumber()));
            line(prefix + "probe_type|" +
                 std::to_string(probe->GetProbeType()));
            line(prefix + "probe_weight|" + num(probe->GetWeighting()));
            line(prefix + "probe_normdir|" +
                 std::to_string(probe->GetNormalDir()));
            line(prefix + "probe_start|" + num(probe->GetStartTime()));
            line(prefix + "probe_stop|" + num(probe->GetStopTime()));
            line(prefix + "probe_oversampling|" +
                 std::to_string(probe->GetOverSampling()));
            line(prefix + "probe_mode_file|" + probe->GetModeFile());
            for (int ny = 0; ny < 3; ++ny) {
                line(prefix + "probe_mode_function|" + std::to_string(ny) + "|" +
                     probe->GetModeFunction(ny));
                line(prefix + "probe_mode_origin|" + std::to_string(ny) + "|" +
                     num(probe->GetModeOrigin(ny)));
            }
            std::vector<double>* samples = probe->GetFDSamples();
            for (size_t index = 0; index < samples->size(); ++index) {
                line(prefix + "probe_fd_sample|" + std::to_string(index) + "|" +
                     num(samples->at(index)));
            }
        }
        if (CSPropDumpBox* dump = prop->ToDumpBox()) {
            line(prefix + "dump_type|" + std::to_string(dump->GetDumpType()));
            line(prefix + "dump_mode|" + std::to_string(dump->GetDumpMode()));
            line(prefix + "dump_file_type|" +
                 std::to_string(dump->GetFileType()));
            line(prefix + "dump_multigrid|" +
                 std::to_string(dump->GetMultiGridLevel()));
            line(prefix + "dump_sub_sampling_enabled|" +
                 (dump->GetSubSampling() ? "1" : "0"));
            line(prefix + "dump_opt_resolution_enabled|" +
                 (dump->GetOptResolution() ? "1" : "0"));
            for (int ny = 0; ny < 3; ++ny) {
                line(prefix + "dump_sub_sampling|" + std::to_string(ny) + "|" +
                     std::to_string(dump->GetSubSampling(ny)));
                line(prefix + "dump_opt_resolution|" + std::to_string(ny) + "|" +
                     num(dump->GetOptResolution(ny)));
            }
        }
        if (CSPropResBox* res = prop->ToResBox()) {
            line(prefix + "resolution_factor|" +
                 std::to_string(res->GetResFactor()));
        }
        if (CSPropUnknown* unknown = prop->ToUnknown()) {
            line(prefix + "unknown_property|" + unknown->GetProperty());
        }
    }

    void primitiveLine(ContinuousStructure& cs, size_t prop_index,
                       size_t prim_index) {
        CSProperties* prop = cs.GetProperty(prop_index);
        CSPrimitives* prim = prop->GetPrimitive(prim_index);
        double bounds[6] = {0, 0, 0, 0, 0, 0};
        bool valid = prim->GetBoundBox(bounds, false);
        std::ostringstream out;
        out << "primitive|" << prop_index << '|' << prim_index << '|'
            << prim->GetTypeName() << '|' << prim->GetPriority() << '|'
            << int(prim->GetCoordinateSystem()) << '|'
            << (valid ? 1 : 0) << '|' << int(prim->GetBoundBoxCoordSystem())
            << '|' << prim->GetDimension();
        for (double value : bounds) out << '|' << num(value);
        line(out.str());
        if (prim->HasTransform()) {
            std::ostringstream matrix;
            matrix << "matrix|" << prop_index << '|' << prim_index;
            double* values = prim->GetTransform()->GetMatrix();
            for (int n = 0; n < 16; ++n) matrix << '|' << num(values[n]);
            line(matrix.str());
        }
    }

    void query(ContinuousStructure& cs, size_t index, const Query& point) {
        double coord[3] = {point.x, point.y, point.z};
        CSPrimitives* found = nullptr;
        CSProperties* prop =
            cs.GetPropertyByCoordPriority(coord, CSProperties::ANY, false, &found);
        int prop_index = -1;
        int prim_index = -1;
        if (prop != nullptr) {
            prop_index = cs.GetIndex(prop);
            for (size_t n = 0; n < prop->GetQtyPrimitives(); ++n) {
                if (prop->GetPrimitive(n) == found) {
                    prim_index = int(n);
                    break;
                }
            }
        }
        std::ostringstream out;
        out << "query|" << index << '|' << num(point.x) << '|' << num(point.y)
            << '|' << num(point.z) << '|' << prop_index << '|' << prim_index;
        line(out.str());
    }
};

void setAxisCoords(CSPrimitives* prim, double x1, double y1, double z1,
                   double x2, double y2, double z2) {
    // Native cylinder axis coordinate layout: x1,x2,y1,y2,z1,z2.
    if (auto* cylinder = prim->ToCylinder()) {
        cylinder->SetCoord(0, x1);
        cylinder->SetCoord(1, x2);
        cylinder->SetCoord(2, y1);
        cylinder->SetCoord(3, y2);
        cylinder->SetCoord(4, z1);
        cylinder->SetCoord(5, z2);
    } else if (auto* shell = prim->ToCylindricalShell()) {
        shell->SetCoord(0, x1);
        shell->SetCoord(1, x2);
        shell->SetCoord(2, y1);
        shell->SetCoord(3, y2);
        shell->SetCoord(4, z1);
        shell->SetCoord(5, z2);
    }
}

void addPrimitive(ContinuousStructure& cs, CSProperties* prop,
                  CSPrimitives* primitive, int priority) {
    // The native primitive constructor already registers the primitive with the
    // property it was constructed for; adding it again would error.
    (void)cs;
    (void)prop;
    primitive->SetPriority(priority);
}

CSProperties* addProperty(ContinuousStructure& cs, CSProperties* prop,
                          const std::string& name, unsigned char r,
                          unsigned char g, unsigned char b) {
    prop->SetName(name);
    prop->SetFillColor(r, g, b, 255);
    prop->SetEdgeColor(r, g, b, 255);
    cs.AddProperty(prop);
    return prop;
}

CSPrimBox* addBox(ContinuousStructure& cs, CSProperties* prop, int priority,
                  std::initializer_list<double> values) {
    auto* box = new CSPrimBox(cs.GetParameterSet(), prop);
    // Native CSPrimBox::SetCoord index layout is interleaved per axis:
    // 0 start.x, 1 stop.x, 2 start.y, 3 stop.y, 4 start.z, 5 stop.z.
    double coords[6];
    int index = 0;
    for (double value : values) coords[index++] = value;
    box->SetCoord(0, coords[0]);
    box->SetCoord(1, coords[3]);
    box->SetCoord(2, coords[1]);
    box->SetCoord(3, coords[4]);
    box->SetCoord(4, coords[2]);
    box->SetCoord(5, coords[5]);
    addPrimitive(cs, prop, box, priority);
    return box;
}

CSPrimBox* addBoxTerms(ContinuousStructure& cs, CSProperties* prop, int priority,
                       const char* x1, const char* y1, const char* z1,
                       const char* x2, const char* y2, const char* z2) {
    auto* box = new CSPrimBox(cs.GetParameterSet(), prop);
    box->SetCoord(0, x1);
    box->SetCoord(1, x2);
    box->SetCoord(2, y1);
    box->SetCoord(3, y2);
    box->SetCoord(4, z1);
    box->SetCoord(5, z2);
    addPrimitive(cs, prop, box, priority);
    return box;
}

void setGrid(ContinuousStructure& cs, double delta_unit,
             const std::vector<double>& x, const std::vector<double>& y,
             const std::vector<double>& z) {
    CSRectGrid* grid = cs.GetGrid();
    grid->SetDeltaUnit(delta_unit);
    for (double value : x) grid->AddDiscLine(0, value);
    for (double value : y) grid->AddDiscLine(1, value);
    for (double value : z) grid->AddDiscLine(2, value);
}

void writeCommonHeader(Report& report, ContinuousStructure& cs,
                       const std::string& library) {
    report.line("format|csxcad-native-report|1");
    report.line("library|" + library);
    report.line("fixture|" + report.fixture);
    report.line("coord_system|" + std::to_string(cs.GetCoordInputType()));
    CSRectGrid* grid = cs.GetGrid();
    report.line("grid_delta_unit|" + num(grid->GetDeltaUnit()));
    for (int direction = 0; direction < 3; ++direction) {
        std::ostringstream out;
        out << "grid|" << direction << '|' << grid->GetQtyLines(direction);
        for (int n = 0; n < grid->GetQtyLines(direction); ++n)
            out << '|' << num(grid->GetLine(direction, n));
        report.line(out.str());
    }
    CSBackgroundMaterial* background = cs.GetBackgroundMaterial();
    report.line("background|" + num(background->GetEpsilon()) + "|" +
                num(background->GetMue()) + "|" + num(background->GetKappa()) +
                "|" + num(background->GetSigma()));
    ParameterSet* parameters = cs.GetParameterSet();
    for (size_t n = 0; n < parameters->GetQtyParameter(); ++n) {
        Parameter* parameter = parameters->GetParameter(n);
        std::ostringstream out;
        out << "parameter|" << n << '|' << parameter->GetName() << '|'
            << (parameter->GetType() == Parameter::Linear ? "Linear" : "Const")
            << '|' << num(parameter->GetValue()) << '|';
        if (auto* linear = dynamic_cast<LinearParameter*>(parameter)) {
            // Native Parameter::GetSweep() is virtual and always false for a
            // constant parameter; the raw bSweep flag is not observable there.
            out << (linear->GetSweep() ? 1 : 0) << '|' << num(linear->GetMin())
                << '|' << num(linear->GetMax()) << '|' << num(linear->GetStep());
        } else {
            out << "na|0|0|0";
        }
        report.line(out.str());
        if (auto* linear = dynamic_cast<LinearParameter*>(parameter)) {
            const double original = linear->GetValue();
            linear->SetValue(linear->GetMin());
            std::ostringstream steps;
            steps << "parameter_steps|" << n;
            int guard = 0;
            do {
                steps << '|' << num(linear->GetValue());
            } while (linear->IncreaseStep() && ++guard < 1000);
            report.line(steps.str());
            // Restore the value the parameter had before the sweep walk so the
            // written XML matches the reported parameter value.
            linear->SetValue(original);
        }
    }
}

void writePropertySections(Report& report, ContinuousStructure& cs) {
    report.line("property_count|" + std::to_string(cs.GetQtyProperties()));
    report.line("primitive_count|" + std::to_string(cs.GetQtyPrimitives()));
    for (size_t index = 0; index < cs.GetQtyProperties(); ++index) {
        report.property(cs, index);
        CSProperties* prop = cs.GetProperty(index);
        for (size_t prim = 0; prim < prop->GetQtyPrimitives(); ++prim)
            report.primitiveLine(cs, index, prim);
    }
}

void writeReportBody(Report& report, ContinuousStructure& cs,
                     const std::vector<Query>& queries,
                     const std::vector<std::string>& expressions) {
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    for (const std::string& line : report.preamble) report.line(line);
    std::string error = cs.Update();
    if (!error.empty()) {
        std::istringstream stream(error);
        std::string line;
        while (std::getline(stream, line))
            report.line("update_warning|" + line);
    }
    writePropertySections(report, cs);
    for (size_t index = 0; index < queries.size(); ++index)
        report.query(cs, index, queries[index]);
    for (const std::string& expression : expressions) {
        ParameterScalar scalar(cs.GetParameterSet(), expression);
        int code = scalar.Evaluate();
        report.line("expression|" + expression + "|" + std::to_string(code) +
                    "|" + num(scalar.GetValue()));
    }
    double* area = cs.GetObjectArea();
    std::ostringstream out;
    out << "object_area";
    for (int n = 0; n < 6; ++n) out << '|' << num(area[n]);
    report.line(out.str());
    report.line("geometry_valid|" + std::to_string(cs.isGeometryValid() ? 1 : 0));
}

std::string finish(Report& report, ContinuousStructure& cs,
                   const std::string& name,
                   const std::vector<Query>& queries,
                   const std::vector<std::string>& expressions,
                   bool read_back = true) {
    const std::string xml_path = out_dir + "/xml/" + name + ".xml";
    if (!cs.Write2XML(xml_path)) {
        std::cerr << "failed to write " << xml_path << std::endl;
        std::exit(2);
    }
    // Manifest counts describe the document that was written (what a reader of
    // the file sees), not the post-read native object, which may drop items the
    // native reader cannot dispatch (e.g. native writes <User-Defined> but only
    // reads <UserDefined>).
    manifest_counts.push_back(
        {cs.GetQtyProperties(), cs.GetQtyPrimitives()});

    Report final_report;
    final_report.fixture = report.fixture;
    final_report.preamble = report.preamble;
    if (read_back) {
        // The reference model must be what *native* CSXCAD sees in the file, so
        // the report is regenerated from a native re-read of the document that
        // was just written. This also captures native write/read asymmetries
        // (float probe times, 6-digit transform arguments, the User-Defined
        // CoordSystem attribute collision, ...).
        ContinuousStructure reloaded;
        std::string read_error = reloaded.ReadFromXML(xml_path);
        final_report.line("read_back|1");
        if (!read_error.empty()) {
            std::istringstream stream(read_error);
            std::string line;
            while (std::getline(stream, line))
                final_report.line("read_warning|" + line);
        }
        writeReportBody(final_report, reloaded, queries, expressions);
    } else {
        // The native reader cannot load a DiscMaterial without its HDF5 file,
        // so the DiscMaterial fixture is reported from the in-memory model and
        // flagged as not read back.
        final_report.line("read_back|0");
        writeReportBody(final_report, cs, queries, expressions);
    }

    const std::string report_path = out_dir + "/native/" + name + ".report";
    std::ofstream file(report_path);
    file << final_report.body.str();
    file.close();
    return name;
}

void writeManifest(const std::vector<std::string>& names) {
    std::ofstream manifest(out_dir + "/xml/manifest.txt");
    manifest << "# Generated by tests/native_gen/generate_native_fixtures.cpp\n";
    manifest << "# columns: fixture|properties|primitives|bytes\n";
    for (size_t index = 0; index < names.size(); ++index) {
        const std::string& name = names[index];
        std::ifstream xml(out_dir + "/xml/" + name + ".xml", std::ios::binary);
        xml.seekg(0, std::ios::end);
        std::streamoff size = xml.tellg();
        manifest << name << '|' << manifest_counts[index].first << '|'
                 << manifest_counts[index].second << '|' << size << '\n';
    }
}

/* ------------------------------------------------------------------ */
/* Fixture builders                                                    */
/* ------------------------------------------------------------------ */

std::string fixtureEmpty() {
    ContinuousStructure cs;
    Report report;
    report.fixture = "01_empty";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture, {}, {});
}

std::string fixtureGridBackgroundParameters() {
    ContinuousStructure cs;
    setGrid(cs, 1e-3, {-10, 0, 10}, {-5, 5}, {0, 2, 4});
    CSBackgroundMaterial* background = cs.GetBackgroundMaterial();
    background->SetEpsilon(2.5);
    background->SetMue(1.5);
    background->SetKappa(0.01);
    background->SetSigma(0.02);
    ParameterSet* parameters = cs.GetParameterSet();
    parameters->AddParameter(new Parameter("width", 4.0));
    parameters->AddParameter(new LinearParameter("radius", 1.5, 0.5, 2.5, 0.5));
    auto* disabled = new Parameter("disabled", 3.0);
    disabled->SetSweep(false);
    parameters->AddParameter(disabled);

    CSProperties* metal = addProperty(cs, new CSPropMetal(cs.GetParameterSet()),
                                      "grid-metal", 220, 40, 40);
    addBoxTerms(cs, metal, 1, "0", "0", "0", "width", "radius", "2");

    Report report;
    report.fixture = "02_grid_background_parameters";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture,
                  {{0.5, 0.5, 0.5}, {5, 5, 5}, {-20, 0, 0}}, {"width/2+radius"});
}

std::string fixtureMaterials() {
    ContinuousStructure cs;
    setGrid(cs, 1.0, {0, 1, 2}, {0, 1, 2}, {0, 1, 2});

    auto* material = new CSPropMaterial(cs.GetParameterSet());
    addProperty(cs, material, "material-iso", 100, 150, 200);
    material->SetEpsilon(2.5);
    material->SetMue(1.5);
    material->SetKappa(0.1);
    material->SetSigma(0.2);
    material->SetDensity(1200.0);
    material->SetEpsilonWeightFunction("2*x+1", 0);
    material->SetKappaWeightFunction("y*z", 0);
    material->SetDensityWeightFunction("rho");
    material->SetAttributeValue("custom", "value & <tag>");
    material->SetVisibility(false);
    addBox(cs, material, 2, {0, 0, 0, 1, 1, 1});

    auto* anisotropic = new CSPropMaterial(cs.GetParameterSet());
    addProperty(cs, anisotropic, "material-aniso", 90, 140, 190);
    anisotropic->SetIsotropy(false);
    for (int ny = 0; ny < 3; ++ny) {
        anisotropic->SetEpsilon(2.0 + ny, ny);
        anisotropic->SetMue(1.1 + ny * 0.1, ny);
    }
    anisotropic->SetEpsilonWeightFunction("y", 1);
    addBox(cs, anisotropic, 1, {0.2, 0.2, 0.2, 0.8, 0.8, 0.8});

    CSProperties* metal = addProperty(cs, new CSPropMetal(cs.GetParameterSet()),
                                      "metal", 200, 200, 200);
    addBox(cs, metal, 3, {0, 0, 1, 1, 1, 2});

    auto* unknown = new CSPropUnknown(cs.GetParameterSet());
    unknown->SetProperty("mystery-tag");
    addProperty(cs, unknown, "unknown", 10, 20, 30);
    addBox(cs, unknown, 0, {0, 0, 0, 0.5, 0.5, 0.5});

    auto* resolution = new CSPropResBox(cs.GetParameterSet());
    resolution->SetResFactor(2);
    addProperty(cs, resolution, "resolution", 50, 50, 160);
    addBox(cs, resolution, 0, {0.1, 0.1, 0.1, 0.9, 0.9, 0.9});

    Report report;
    report.fixture = "03_materials";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture, {{0.5, 0.5, 0.5}, {0.95, 0.95, 0.4}},
                  {});
}

std::string fixtureDispersive() {
    ContinuousStructure cs;
    setGrid(cs, 1.0, {0, 1, 2}, {0, 1, 2}, {0, 1, 2});


    auto* debye = new CSPropDebyeMaterial(cs.GetParameterSet());
    debye->SetDispersionOrder(2);
    addProperty(cs, debye, "debye", 60, 160, 90);
    debye->SetEpsilon(1.5);
    debye->SetEpsDelta(0, 0.5);
    debye->SetEpsRelaxTime(0, 1e-12);
    debye->SetEpsDelta(1, 0.25);
    debye->SetEpsRelaxTime(1, 2e-12);
    debye->SetEpsDeltaWeightFunction(0, "y*2", 0);
    debye->SetEpsRelaxTimeWeightFunction(0, "z+1", 0);
    addBox(cs, debye, 1, {0.1, 0.1, 0.1, 0.9, 0.9, 0.9});

    auto* lorentz = new CSPropLorentzMaterial(cs.GetParameterSet());
    lorentz->SetDispersionOrder(2);
    addProperty(cs, lorentz, "lorentz", 190, 120, 40);
    lorentz->SetEpsilon(2.0);
    lorentz->SetEpsPlasmaFreq(0, 1e9);
    lorentz->SetEpsLorPoleFreq(0, 2e9);
    lorentz->SetEpsRelaxTime(0, 1e-9);
    lorentz->SetMuePlasmaFreq(0, 3e9);
    lorentz->SetMueLorPoleFreq(0, 4e9);
    lorentz->SetMueRelaxTime(0, 2e-9);
    lorentz->SetEpsPlasmaFreq(1, 1.5e9);
    lorentz->SetEpsPlasmaFreqWeightFunction(0, "x+1", 0);
    addBox(cs, lorentz, 2, {0.2, 0.2, 0.2, 0.8, 0.8, 0.8});

    Report report;
    report.fixture = "04_dispersive";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture, {{0.5, 0.5, 0.5}, {0.05, 0.5, 0.5}},
                  {});
}

std::string fixtureSheetsLumpedAbc() {
    ContinuousStructure cs;
    setGrid(cs, 1.0, {0, 1, 2, 3}, {0, 1, 2}, {0, 1, 2});

    auto* sheet = new CSPropConductingSheet(cs.GetParameterSet());
    addProperty(cs, sheet, "sheet", 210, 170, 40);
    sheet->SetConductivity(56e6);
    sheet->SetThickness(18e-6);
    addBox(cs, sheet, 5, {1, 0, 0, 1, 1, 1});

    auto* lumped = new CSPropLumpedElement(cs.GetParameterSet());
    addProperty(cs, lumped, "lumped", 40, 120, 220);
    lumped->SetDirection(1);
    lumped->SetCaps(true);
    lumped->SetResistance(50.0);
    lumped->SetCapacity(1e-12);
    lumped->SetInductance(1e-9);
    lumped->SetLEtype(CSPropLumpedElement::SERIES);
    addBox(cs, lumped, 1, {1.5, 0.5, 0.5, 1.5, 1.5, 1.5});

    auto* abc = new CSPropAbsorbingBC(cs.GetParameterSet());
    addProperty(cs, abc, "abc", 70, 70, 70);
    abc->SetNormalSignPositive(false);
    abc->SetPhaseVelocity(3e8);
    abc->SetAbsorbingBoundaryType(CSPropAbsorbingBC::MUR_1ST);
    addBox(cs, abc, 0, {0, 0, 0, 0.5, 2, 2});

    Report report;
    report.fixture = "05_sheets_lumped_abc";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture, {{1.5, 1.0, 1.0}, {0.25, 1, 1}}, {});
}

std::string fixtureExcitationProbeDump() {
    ContinuousStructure cs;
    setGrid(cs, 1e-3, {0, 1, 2}, {0, 1, 2}, {0, 1, 2});

    auto* excitation = new CSPropExcitation(cs.GetParameterSet(), 1);
    addProperty(cs, excitation, "excitation", 250, 100, 0);
    excitation->SetExcitType(1);
    excitation->SetExcitation(1.0, 0);
    excitation->SetExcitation(0.0, 1);
    excitation->SetExcitation(0.0, 2);
    excitation->SetPropagationDir(0.0, 0);
    excitation->SetPropagationDir(0.0, 1);
    excitation->SetPropagationDir(1.0, 2);
    excitation->SetWeightFunction("sin(x)", 0);
    excitation->SetWeightFunction("cos(y)", 1);
    excitation->SetWeightFunction("1", 2);
    excitation->SetFrequency(2.4e9);
    excitation->SetDelay(1e-9);
    excitation->SetEnabled(false);
    excitation->SetWeightOrigin(1.0, 2.0, 3.0);
    addBox(cs, excitation, 0, {0.5, 0.5, 0, 0.5, 0.5, 1});

    auto* weight_file = new CSPropExcitation(cs.GetParameterSet(), 2);
    addProperty(cs, weight_file, "excitation-weight-file", 250, 150, 50);
    weight_file->SetExcitType(0);
    weight_file->SetExcitation(0.0, 0);
    weight_file->SetExcitation(1.0, 1);
    weight_file->SetExcitation(0.0, 2);
    weight_file->SetWeightFile("mode_profile.h5");
    addBox(cs, weight_file, 0, {1.5, 0.5, 0, 1.5, 0.5, 1});

    auto* probe = new CSPropProbeBox(cs.GetParameterSet());
    addProperty(cs, probe, "probe", 60, 180, 240);
    probe->SetNumber(2);
    probe->SetProbeType(1);
    probe->SetWeighting(2.5);
    probe->SetNormalDir(1);
    probe->SetStartTime(0.1);
    probe->SetStopTime(2.5);
    probe->SetOverSampling(4);
    probe->AddFDSample(1e9);
    probe->AddFDSample(2e9);
    probe->AddFDSample(3e9);
    probe->SetModeFunction(0, "sin(x)");
    probe->SetModeFunction(1, "cos(y)");
    probe->SetModeFunction(2, "1");
    probe->SetModeOrigin(1.0, 2.0, 3.0);
    addBox(cs, probe, 0, {0, 0, 0, 1, 1, 1});

    auto* probe_mode_file = new CSPropProbeBox(cs.GetParameterSet());
    addProperty(cs, probe_mode_file, "probe-mode-file", 80, 200, 120);
    probe_mode_file->SetNumber(10);
    probe_mode_file->SetModeFile("mode.h5");
    probe_mode_file->SetModeOrigin(1.0, 2.0, 3.0);
    addBox(cs, probe_mode_file, 0, {0, 0, 1, 1, 1, 2});

    auto* dump = new CSPropDumpBox(cs.GetParameterSet());
    addProperty(cs, dump, "dump", 200, 60, 200);
    dump->SetNumber(3);
    dump->SetDumpType(10);
    dump->SetDumpMode(2);
    dump->SetFileType(1);
    dump->SetMultiGridLevel(2);
    dump->SetSubSampling(true);
    dump->SetSubSampling(0, 2);
    dump->SetSubSampling(1, 3);
    dump->SetSubSampling(2, 4);
    dump->SetOptResolution(true);
    dump->SetOptResolution(0, 0.1);
    dump->SetOptResolution(1, 0.2);
    dump->SetOptResolution(2, 0.3);
    dump->AddFDSample(1e9);
    dump->AddFDSample(2e9);
    addBox(cs, dump, 0, {0, 0, 0, 2, 2, 2});

    Report report;
    report.fixture = "06_excitation_probe_dump";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture, {{0.5, 0.5, 0.5}, {1.5, 0.5, 0.5}},
                  {});
}

std::string fixturePrimitives() {
    ContinuousStructure cs;
    setGrid(cs, 1.0, {-2, -1, 0, 1, 2}, {-2, -1, 0, 1, 2}, {-2, -1, 0, 1, 2});

    CSProperties* metal = addProperty(cs, new CSPropMetal(cs.GetParameterSet()),
                                      "primitives", 180, 180, 60);

    auto* point = new CSPrimPoint(cs.GetParameterSet(), metal);
    point->SetCoord(0, 1.5);
    point->SetCoord(1, 2.5);
    point->SetCoord(2, 3.5);
    addPrimitive(cs, metal, point, 0);

    addBox(cs, metal, 3, {1, 2, 3, 4, 5, 6});

    auto* multi = new CSPrimMultiBox(cs.GetParameterSet(), metal);
    // Native MultiBox coordinate layout per box: x1,x2,y1,y2,z1,z2.
    for (double value : {0.0, 1.0, 0.0, 1.0, 0.0, 1.0}) multi->AddCoord(value);
    for (double value : {2.0, 3.0, 2.0, 3.0, 2.0, 3.0}) multi->AddCoord(value);
    addPrimitive(cs, metal, multi, 2);

    auto* sphere = new CSPrimSphere(cs.GetParameterSet(), metal);
    sphere->SetCenter(1.0, 2.0, 3.0);
    sphere->SetRadius(0.5);
    addPrimitive(cs, metal, sphere, 0);

    auto* spherical_shell = new CSPrimSphericalShell(cs.GetParameterSet(), metal);
    spherical_shell->SetCenter(1.0, 2.0, 3.0);
    spherical_shell->SetRadius(2.0);
    spherical_shell->SetShellWidth(0.2);
    addPrimitive(cs, metal, spherical_shell, 0);

    auto* cylinder = new CSPrimCylinder(cs.GetParameterSet(), metal);
    setAxisCoords(cylinder, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    cylinder->SetRadius(1.5);
    addPrimitive(cs, metal, cylinder, 0);

    auto* cylinder_shell = new CSPrimCylindricalShell(cs.GetParameterSet(), metal);
    setAxisCoords(cylinder_shell, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0);
    cylinder_shell->SetRadius(2.0);
    cylinder_shell->SetShellWidth(0.3);
    addPrimitive(cs, metal, cylinder_shell, 0);

    auto* polygon = new CSPrimPolygon(cs.GetParameterSet(), metal);
    polygon->SetNormDir(2);
    polygon->SetElevation(1.5);
    polygon->AddCoord(0.0);
    polygon->AddCoord(0.0);
    polygon->AddCoord(1.0);
    polygon->AddCoord(0.0);
    polygon->AddCoord(0.5);
    polygon->AddCoord(1.0);
    addPrimitive(cs, metal, polygon, 0);

    auto* lin_poly = new CSPrimLinPoly(cs.GetParameterSet(), metal);
    lin_poly->SetNormDir(1);
    lin_poly->SetElevation(0.5);
    lin_poly->SetLength(2.0);
    lin_poly->AddCoord(0.0);
    lin_poly->AddCoord(0.0);
    lin_poly->AddCoord(1.0);
    lin_poly->AddCoord(0.0);
    lin_poly->AddCoord(0.5);
    lin_poly->AddCoord(1.0);
    addPrimitive(cs, metal, lin_poly, 0);

    auto* rot_poly = new CSPrimRotPoly(cs.GetParameterSet(), metal);
    rot_poly->SetNormDir(2);
    rot_poly->SetRotAxisDir(0);
    rot_poly->SetAngle(0, 0.1);
    rot_poly->SetAngle(1, M_PI);
    rot_poly->AddCoord(0.0);
    rot_poly->AddCoord(0.0);
    rot_poly->AddCoord(1.0);
    rot_poly->AddCoord(0.0);
    rot_poly->AddCoord(0.5);
    rot_poly->AddCoord(1.0);
    addPrimitive(cs, metal, rot_poly, 0);

    auto* polyhedron = new CSPrimPolyhedron(cs.GetParameterSet(), metal);
    polyhedron->AddVertex(0.0, 0.0, 0.0);
    polyhedron->AddVertex(1.0, 0.0, 0.0);
    polyhedron->AddVertex(0.0, 1.0, 0.0);
    polyhedron->AddVertex(0.0, 0.0, 1.0);
    int face0[] = {0, 1, 2};
    int face1[] = {0, 1, 3};
    int face2[] = {0, 2, 3};
    int face3[] = {1, 2, 3};
    polyhedron->AddFace(3, face0);
    polyhedron->AddFace(3, face1);
    polyhedron->AddFace(3, face2);
    polyhedron->AddFace(3, face3);
    addPrimitive(cs, metal, polyhedron, 0);

    auto* curve = new CSPrimCurve(cs.GetParameterSet(), metal);
    { double curve_point[3] = {0.0, 4.0, 8.0}; curve->AddPoint(curve_point); }
    { double curve_point[3] = {1.0, 5.0, 9.0}; curve->AddPoint(curve_point); }
    { double curve_point[3] = {2.0, 6.0, 10.0}; curve->AddPoint(curve_point); }
    { double curve_point[3] = {3.0, 7.0, 11.0}; curve->AddPoint(curve_point); }
    addPrimitive(cs, metal, curve, 0);

    auto* wire = new CSPrimWire(cs.GetParameterSet(), metal);
    wire->SetWireRadius(0.25);
    { double wire_point[3] = {0.0, 0.0, 0.0}; wire->AddPoint(wire_point); }
    { double wire_point[3] = {1.0, 0.0, 0.0}; wire->AddPoint(wire_point); }
    { double wire_point[3] = {2.0, 0.0, 0.0}; wire->AddPoint(wire_point); }
    addPrimitive(cs, metal, wire, 0);

    auto* user = new CSPrimUserDefined(cs.GetParameterSet(), metal);
    user->SetCoordSystem(CSPrimUserDefined::CARESIAN_SYSTEM);
    user->SetCoordShift(0, 1.0);
    user->SetCoordShift(1, 0.0);
    user->SetCoordShift(2, 0.0);
    user->SetFunction("x>1 & y>0");
    addPrimitive(cs, metal, user, 0);

    // Coordinate system on a primitive itself (native writes CoordSystem on
    // the primitive when it is not the mesh type).
    auto* cylindrical_box = new CSPrimBox(cs.GetParameterSet(), metal);
    cylindrical_box->SetCoordinateSystem(CYLINDRICAL);
    // Box in cylindrical coordinates (rho, alpha, z), native interleaved layout.
    cylindrical_box->SetCoord(0, 0.0);
    cylindrical_box->SetCoord(1, 1.0);
    cylindrical_box->SetCoord(2, 0.0);
    cylindrical_box->SetCoord(3, M_PI / 2.0);
    cylindrical_box->SetCoord(4, 0.0);
    cylindrical_box->SetCoord(5, 1.0);
    addPrimitive(cs, metal, cylindrical_box, 4);

    Report report;
    report.fixture = "07_primitives";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture,
                  {{1.5, 2.5, 3.5}, {1.5, 3.5, 4.5}, {0.5, 0.0, 0.5},
                   {0.7, 0.7, 0.7}, {0.5, 0.5, 0.5}},
                  {});
}

std::string fixtureTransforms() {
    ContinuousStructure cs;
    setGrid(cs, 1.0, {-2, -1, 0, 1, 2}, {-2, -1, 0, 1, 2}, {-2, -1, 0, 1, 2});
    ParameterSet* parameters = cs.GetParameterSet();
    parameters->AddParameter(new Parameter("dx", 2.0));

    CSProperties* metal = addProperty(cs, new CSPropMetal(cs.GetParameterSet()),
                                      "transforms", 180, 80, 180);

    // Translate with an expression argument.
    CSPrimBox* translated = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    translated->GetTransform()->Translate(std::string("dx,0,0"), true);

    CSPrimBox* scaled = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    scaled->GetTransform()->Scale(2.0, true);

    CSPrimBox* scaled3 = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    scaled3->GetTransform()->Scale(std::string("1,2,3"), true);

    CSPrimBox* rotated_origin = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    rotated_origin->GetTransform()->RotateOrigin(
        std::string("0,1,1,1.5707963267948966"), true);

    CSPrimBox* rotated_x = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    rotated_x->GetTransform()->RotateX(M_PI / 2.0, true);

    CSPrimBox* rotated_y = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    rotated_y->GetTransform()->RotateY(M_PI / 4.0, true);

    CSPrimBox* rotated_z = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    rotated_z->GetTransform()->RotateZ(M_PI / 2.0, true);

    CSPrimBox* degree = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    degree->GetTransform()->SetAngleDegree();
    degree->GetTransform()->RotateZ(90.0, true);

    CSPrimBox* matrix = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    matrix->GetTransform()->SetMatrix(std::string("1,0,0,0,0,1,0,0,0,0,1,0,0.5,0.5,0.5,1"));
    matrix->GetTransform()->Scale(2.0, true);

    CSPrimBox* pre = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    pre->GetTransform()->Translate(std::string("1,0,0"));
    pre->GetTransform()->SetPreMultiply();
    pre->GetTransform()->RotateZ(M_PI / 2.0, true);

    CSPrimBox* post = addBox(cs, metal, 0, {0, 0, 0, 1, 1, 1});
    post->GetTransform()->Translate(std::string("1,0,0"));
    post->GetTransform()->SetPostMultiply();
    post->GetTransform()->RotateZ(M_PI / 2.0, true);

    Report report;
    report.fixture = "08_transforms";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));

    // Operation-order scenarios for RM-034. These use a fresh standalone
    // transform per scenario so the Rust side can reproduce them exactly.
    {
        CSTransform post_order(parameters);
        post_order.Translate(std::string("1,0,0"));
        post_order.RotateZ(M_PI / 2.0);
        double* matrix_values = post_order.GetMatrix();
        std::ostringstream matrix_line;
        matrix_line << "tscenario|post_translate_rotate";
        for (int n = 0; n < 16; ++n) matrix_line << '|' << num(matrix_values[n]);
        report.preamble.push_back(matrix_line.str());
    }
    {
        CSTransform pre_order(parameters);
        pre_order.Translate(std::string("1,0,0"));
        pre_order.SetPreMultiply();
        pre_order.RotateZ(M_PI / 2.0);
        double* matrix_values = pre_order.GetMatrix();
        std::ostringstream matrix_line;
        matrix_line << "tscenario|pre_translate_rotate";
        for (int n = 0; n < 16; ++n) matrix_line << '|' << num(matrix_values[n]);
        report.preamble.push_back(matrix_line.str());
    }
    {
        CSTransform rotation(parameters);
        rotation.RotateOrigin(std::string("0,1,1,1.5707963267948966"), true);
        double* matrix_values = rotation.GetMatrix();
        std::ostringstream matrix_line;
        matrix_line << "tscenario|rotate_origin";
        for (int n = 0; n < 16; ++n) matrix_line << '|' << num(matrix_values[n]);
        report.preamble.push_back(matrix_line.str());
    }
    {
        CSTransform degree(parameters);
        degree.SetAngleDegree();
        degree.RotateX(90.0, true);
        double* matrix_values = degree.GetMatrix();
        std::ostringstream matrix_line;
        matrix_line << "tscenario|degree_rotate_x";
        for (int n = 0; n < 16; ++n) matrix_line << '|' << num(matrix_values[n]);
        report.preamble.push_back(matrix_line.str());
    }
    {
        CSTransform matrix_transform(parameters);
        matrix_transform.SetMatrix(std::string("1,0,0,0,0,1,0,0,0,0,1,0,0.5,0.5,0.5,1"));
        matrix_transform.Scale(std::string("1,2,3"), true);
        double* matrix_values = matrix_transform.GetMatrix();
        std::ostringstream matrix_line;
        matrix_line << "tscenario|matrix_then_scale3";
        for (int n = 0; n < 16; ++n) matrix_line << '|' << num(matrix_values[n]);
        report.preamble.push_back(matrix_line.str());
    }
    {
        CSTransform inverted(parameters);
        inverted.Translate(std::string("1,2,3"));
        inverted.RotateZ(M_PI / 2.0);
        inverted.Invert();
        double* matrix_values = inverted.GetMatrix();
        std::ostringstream matrix_line;
        matrix_line << "tscenario|invert";
        for (int n = 0; n < 16; ++n) matrix_line << '|' << num(matrix_values[n]);
        report.preamble.push_back(matrix_line.str());
    }

    return finish(report, cs, report.fixture,
                  {{0.5, 0.5, 0.5}, {2.5, 0.5, 0.5}, {1.5, 0.5, 0.5}},
                  {"dx*2", "1/0", "sqrt(-1)", "log(-1)", "asin(2)", "missing+1",
                   "2*", "2**3", "(1+2", "scale(1,2)", "1e-3+dx"});
}

std::string fixtureCylindrical() {
    ContinuousStructure cs;
    cs.SetCoordInputType(CYLINDRICAL);
    setGrid(cs, 1.0, {0, 1, 2}, {0, M_PI / 2.0, M_PI}, {0, 1, 2});

    auto* material = new CSPropMaterial(cs.GetParameterSet());
    addProperty(cs, material, "cylindrical-material", 120, 160, 60);
    addBox(cs, material, 1, {0, 0, 0, 1, M_PI / 2.0, 1});

    CSProperties* metal = addProperty(cs, new CSPropMetal(cs.GetParameterSet()),
                                      "cylindrical-metal", 200, 120, 120);
    auto* cylinder = new CSPrimCylinder(cs.GetParameterSet(), metal);
    setAxisCoords(cylinder, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    cylinder->SetRadius(0.5);
    addPrimitive(cs, metal, cylinder, 0);

    Report report;
    report.fixture = "09_cylindrical";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture,
                  {{0.5, 0.5, 0.5}, {0.5, 2.0, 0.5}, {1.5, 0.5, 0.5}}, {});
}

std::string fixtureOpenEmsWrapper() {
    ContinuousStructure* cs = new ContinuousStructure();
    setGrid(*cs, 1e-3, {0, 1, 2}, {0, 1, 2}, {0, 1, 2});
    CSProperties* metal = addProperty(*cs, new CSPropMetal(cs->GetParameterSet()),
                                      "wrapper-metal", 120, 120, 200);
    addBox(*cs, metal, 0, {0, 0, 0, 1, 1, 1});

    openEMS solver;
    solver.SetCSX(cs);
    solver.SetNumberOfTimeSteps(100);
    solver.SetEndCriteria(1e-5);
    solver.SetGaussExcite(1e9, 0.5e9);
    solver.Set_BC_Type(0, 3);
    solver.Set_BC_PML(0, 8);
    solver.Set_BC_Type(1, 2);
    const std::string path = out_dir + "/xml/10_openems_wrapper.xml";
    if (!solver.Write2XML(path)) {
        std::cerr << "failed to write " << path << std::endl;
        std::exit(2);
    }

    Report report;
    report.fixture = "10_openems_wrapper";
    // The wrapper file is written by openEMS; the report describes the
    // ContinuousStructure it contains (extracted natively by the solver).
    writeCommonHeader(report, *solver.GetCSX(),
                      ContinuousStructure::GetInfoLine(true));
    std::string name = finish(report, *solver.GetCSX(), report.fixture,
                              {{0.5, 0.5, 0.5}}, {});
    // finish() wrote xml/10_openems_wrapper.xml from the bare structure; the
    // openEMS wrapper is the authoritative fixture, so restore it.
    if (!solver.Write2XML(path)) {
        std::cerr << "failed to rewrite " << path << std::endl;
        std::exit(2);
    }
    return name;
}

std::string fixtureDiscMaterial() {
    ContinuousStructure cs;
    setGrid(cs, 1.0, {0, 1, 2}, {0, 1, 2}, {0, 1, 2});

    auto* disc = new CSPropDiscMaterial(cs.GetParameterSet());
    addProperty(cs, disc, "disc", 100, 200, 100);
    disc->SetFilename("materials.h5");
    disc->SetFileType(0);
    disc->SetScale(1e-3);
    disc->SetUseDataBaseForBackground(false);
    disc->SetEpsilon(1.0);
    disc->GetTransform()->Translate(std::string("1,0,0"));
    addBox(cs, disc, 0, {0, 0, 0, 1, 1, 1});

    Report report;
    report.fixture = "11_disc_material";
    writeCommonHeader(report, cs, ContinuousStructure::GetInfoLine(true));
    return finish(report, cs, report.fixture, {{0.5, 0.5, 0.5}}, {}, false);
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::cerr << "usage: " << argv[0] << " <output-dir>" << std::endl;
        return 1;
    }
    out_dir = argv[1];
    std::vector<std::string> names;
    names.push_back(fixtureEmpty());
    names.push_back(fixtureGridBackgroundParameters());
    names.push_back(fixtureMaterials());
    names.push_back(fixtureDispersive());
    names.push_back(fixtureSheetsLumpedAbc());
    names.push_back(fixtureExcitationProbeDump());
    names.push_back(fixturePrimitives());
    names.push_back(fixtureTransforms());
    names.push_back(fixtureCylindrical());
    names.push_back(fixtureOpenEmsWrapper());
    names.push_back(fixtureDiscMaterial());
    writeManifest(names);
    for (const std::string& name : names) std::cout << "wrote " << name << '\n';
    return 0;
}



