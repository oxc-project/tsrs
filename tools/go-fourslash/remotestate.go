package fourslash

import (
	"maps"
	"slices"
	"testing"

	"github.com/microsoft/TypeScript/tsc/internal/project"
	"github.com/microsoft/TypeScript/tsc/internal/tspath"
)

// These read-only views provide the subset of the project API used by the
// unchanged Go state diff. Handles preserve Rust program/source-file identity.
type remoteSnapshot struct {
	ProjectCollection *remoteProjectCollection
}

type remoteProjectCollection struct {
	Items    []*remoteProject
	Defaults map[tspath.Path]project.ID
	Registry *remoteConfigFileRegistry
}

func (c *remoteProjectCollection) Projects() []*remoteProject {
	return c.Items
}

func (c *remoteProjectCollection) GetDefaultProject(path tspath.Path) *remoteProject {
	for _, p := range c.Items {
		if p.Name == c.Defaults[path] {
			return p
		}
	}
	return nil
}

func (c *remoteProjectCollection) ConfigFileRegistry() *remoteConfigFileRegistry {
	return c.Registry
}

type remoteProject struct {
	Name    project.ID
	Program *remoteProgram
}

func (p *remoteProject) ID() project.ID             { return p.Name }
func (p *remoteProject) GetProgram() *remoteProgram { return p.Program }

type remoteProgram struct {
	Handle string
	Files  []*remoteSourceFile
}

func (p *remoteProgram) GetSourceFiles() []*remoteSourceFile { return p.Files }
func (p *remoteProgram) GetSourceFileByPath(path tspath.Path) *remoteSourceFile {
	for _, file := range p.Files {
		if file.PathName == path {
			return file
		}
	}
	return nil
}

type remoteSourceFile struct {
	Handle   string
	Name     string
	PathName tspath.Path
}

func (f *remoteSourceFile) FileName() string  { return f.Name }
func (f *remoteSourceFile) Path() tspath.Path { return f.PathName }

type remoteConfigFileRegistry struct {
	Configs   map[tspath.Path]*remoteConfigEntry
	FileNames map[tspath.Path]*project.TestConfigFileNamesEntry
}

type remoteConfigEntry struct {
	FileName           string
	RetainingProjects  []project.ID
	RetainingOpenFiles []tspath.Path
	RetainingConfigs   []tspath.Path
}

func (r *remoteConfigFileRegistry) GetTestConfigEntry(path tspath.Path) *project.TestConfigEntry {
	if r == nil || r.Configs[path] == nil {
		return nil
	}
	e := r.Configs[path]
	return &project.TestConfigEntry{
		FileName:           e.FileName,
		RetainingProjects:  slices.Values(e.RetainingProjects),
		RetainingOpenFiles: slices.Values(e.RetainingOpenFiles),
		RetainingConfigs:   slices.Values(e.RetainingConfigs),
	}
}

func (r *remoteConfigFileRegistry) ForEachTestConfigEntry(fn func(tspath.Path, *project.TestConfigEntry)) {
	if r != nil {
		for path := range r.Configs {
			fn(path, r.GetTestConfigEntry(path))
		}
	}
}

func (r *remoteConfigFileRegistry) GetTestConfigFileNamesEntry(path tspath.Path) *project.TestConfigFileNamesEntry {
	if r == nil {
		return nil
	}
	return r.FileNames[path]
}

func (r *remoteConfigFileRegistry) ForEachTestConfigFileNamesEntry(fn func(tspath.Path, *project.TestConfigFileNamesEntry)) {
	if r != nil {
		for path, entry := range r.FileNames {
			fn(path, entry)
		}
	}
}

func (f *FourslashTest) remoteSnapshot(t *testing.T) *remoteSnapshot {
	t.Helper()
	var snapshot remoteSnapshot
	f.client.SubprocessState(t, slices.Collect(maps.Keys(f.openFiles)), &snapshot)

	// Go's baseline compares pointers. Intern handles against the previous snapshot,
	// then share identities across projects in the new snapshot as well.
	programs := make(map[string]*remoteProgram)
	files := make(map[string]*remoteSourceFile)
	for _, program := range f.stateBaseline.serializedProjects {
		if program != nil {
			programs[program.Handle] = program
			for _, file := range program.Files {
				files[file.Handle] = file
			}
		}
	}
	for _, p := range snapshot.ProjectCollection.Items {
		if p.Program == nil {
			continue
		}
		if old := programs[p.Program.Handle]; old != nil {
			p.Program = old
			continue
		}
		programs[p.Program.Handle] = p.Program
		for i, file := range p.Program.Files {
			if old := files[file.Handle]; old != nil {
				p.Program.Files[i] = old
			} else {
				files[file.Handle] = file
			}
		}
	}
	return &snapshot
}
