import MtrxFFI
import SwiftUI

/// Which sections a space's list shows on this phone, ticked, and their
/// order, dragged; Show all and Hide all. The desktop's arrangement is not
/// changed.
struct ShowSheet: View {
    let item: SpaceItem
    @EnvironmentObject private var session: SessionModel
    @Environment(\.dismiss) private var dismiss

    private var layout: SectionLayout { session.sectionLayouts[item.entry] }
    private var keys: [String] { item.sections.map(\.key) }

    var body: some View {
        let byKey = Dictionary(item.sections.map { ($0.key, $0) }, uniquingKeysWith: { first, _ in first })
        NavigationStack {
            List {
                Section {
                    HStack {
                        Button("Show all") { change { $0.setAllShown(keys, true) } }
                        Spacer()
                        Button("Hide all") { change { $0.setAllShown(keys, false) } }
                    }
                    .buttonStyle(.borderless)
                }
                Section {
                    ForEach(layout.arranged(keys), id: \.self) { key in
                        if let section = byKey[key] {
                            row(section)
                        }
                    }
                    .onMove { offsets, destination in
                        change { $0.move(keys, from: offsets, to: destination) }
                    }
                } footer: {
                    Text("Drag to reorder. Only on this phone: the desktop keeps its own.")
                }
            }
            .environment(\.editMode, .constant(.active))
            .navigationTitle("Show in \(item.name)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func row(_ section: RoomSection) -> some View {
        let shown = layout.shows(section.key)
        return Button {
            change { $0.setShown(section.key, !shown) }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: shown ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundColor(shown ? .accentColor : .secondary)
                Text(RoomLists.title(section))
                    .foregroundColor(.primary)
                Spacer()
                Text(String(section.roomIds.count))
                    .foregroundColor(.secondary)
                    .monospacedDigit()
            }
        }
        .accessibilityLabel(RoomLists.title(section))
        .accessibilityValue(shown ? "Shown" : "Hidden")
    }

    private func change(_ change: (inout SectionLayout) -> Void) {
        session.sectionLayouts.change(item.entry, change)
    }
}
